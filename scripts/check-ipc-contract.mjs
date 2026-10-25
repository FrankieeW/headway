#!/usr/bin/env node
//! Verifies the TypeScript <-> Rust Tauri IPC contract without running either side.
//
// The frontend side is parsed with the TypeScript compiler API (real AST, not regex), the Rust
// side by reading `generate_handler![...]` and `#[tauri::command]` signatures. This catches the
// drift that `cargo test` cannot see (a renamed/removed command or argument on either side) and
// runs in the CI `frontend` job and in the release `verify` job.
//
// Contract enforced:
//   - every `invoke("<name>")` in `src/lib/api/**` must be registered in `generate_handler!` (error)
//   - every registered handler must have exactly one `#[tauri::command]` function (error)
//   - literal argument objects must only use keys that map to the Rust parameters (error)
//   - a handler that no frontend binding invokes is reported as a warning, not a failure
//     (commands can be Rust-only, e.g. used by the CLI or by tests)
//   - dynamic/non-literal command names and argument objects are reported and skipped
//
// Injected parameters (`State<…>`, `AppHandle<…>`, `Window<…>`, `WebviewWindow<…>`, `App<…>`)
// are not IPC arguments and are excluded from the comparison.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import ts from "typescript";

const rootArg = process.argv.find((arg) => arg.startsWith("--root="));
const root = rootArg ? resolve(rootArg.slice("--root=".length)) : resolve(import.meta.dirname, "..");
const API_DIR = join(root, "src/lib/api");
const TAURI_SRC = join(root, "src-tauri/src");
const LIB_RS = join(TAURI_SRC, "lib.rs");

/** Files under `dir` (recursive) whose path ends with one of `suffixes`, sorted for stable output. */
function walk(dir, suffixes) {
  const found = [];
  for (const entry of readdirSync(dir).sort()) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      found.push(...walk(path, suffixes));
    } else if (suffixes.some((suffix) => path.endsWith(suffix))) {
      found.push(path);
    }
  }
  return found;
}

/** `commands::apps::list_apps` -> `list_apps`. */
function commandName(pathEntry) {
  return pathEntry.trim().split("::").at(-1) ?? "";
}

/** Handlers registered with `tauri::generate_handler![...]` in `src-tauri/src/lib.rs`. */
function readHandlerList() {
  const text = readFileSync(LIB_RS, "utf8");
  const match = /generate_handler!\s*\[([\s\S]*?)\]/.exec(text);
  if (!match) {
    throw new Error(`cannot find generate_handler![...] in ${relative(root, LIB_RS)}`);
  }
  return match[1]
    .split(",")
    .map(commandName)
    .filter(Boolean);
}

/** Splits a parameter list on top-level commas (`State<'_, A>, b: Vec<T>`, …). */
function splitParams(params) {
  const parts = [];
  let depth = 0;
  let current = "";
  for (const char of params) {
    if ("<([{".includes(char)) depth += 1;
    if (">)]}".includes(char)) depth -= 1;
    if (char === "," && depth === 0) {
      parts.push(current);
      current = "";
    } else {
      current += char;
    }
  }
  if (current.trim()) parts.push(current);
  return parts;
}

/** Index of the `)` matching the `(` at `open`. */
function closingParen(text, open) {
  let depth = 0;
  for (let index = open; index < text.length; index += 1) {
    if (text[index] === "(") depth += 1;
    if (text[index] === ")") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  throw new Error("unbalanced parentheses while reading a #[tauri::command] signature");
}

const INJECTED = /^(?:State|AppHandle|Window|WebviewWindow|App|Manager|Webview)\b/;

/** Test sources are not part of the shipped command surface (they also quote the attribute). */
function isTestSource(file) {
  const path = file.split("\\").join("/");
  return (
    path.includes("/ipc_tests/") ||
    path.includes("/tests/") ||
    /(?:^|\/)tests?\.rs$/.test(path) ||
    /_tests\.rs$/.test(path)
  );
}

/** `#[tauri::command]` functions: name -> { params: string[], file, line }. */
function readRustCommands() {
  const commands = new Map();
  for (const file of walk(TAURI_SRC, [".rs"])) {
    if (isTestSource(file)) continue;
    const text = readFileSync(file, "utf8");
    const attribute = /#\[tauri::command[^\]]*\]/g;
    for (const match of text.matchAll(attribute)) {
      const after = text.slice(match.index + match[0].length);
      const signature = /^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*(?:<[^>]*>)?\s*\(/
        .exec(after);
      if (!signature) {
        throw new Error(`#[tauri::command] without a function in ${relative(root, file)}`);
      }
      const name = signature[1];
      const open = match.index + match[0].length + signature[0].length - 1;
      const params = splitParams(text.slice(open + 1, closingParen(text, open)))
        .map((param) => param.trim())
        .filter(Boolean)
        .map((param) => {
          const parts = /^(?:mut\s+)?([A-Za-z0-9_]+)\s*:\s*([\s\S]+)$/.exec(param);
          if (!parts) return null;
          return { name: parts[1], type: parts[2].trim() };
        })
        .filter(Boolean)
        .filter((param) => !INJECTED.test(param.type))
        .map((param) => param.name);
      const line = text.slice(0, match.index).split("\n").length;
      if (commands.has(name)) {
        throw new Error(`duplicate #[tauri::command] ${name} (${relative(root, file)}:${line})`);
      }
      commands.set(name, { params, file, line });
    }
  }
  return commands;
}

/** `invoke("<name>", { … })` calls from the typed API layer. */
function readInvocations() {
  const calls = [];
  for (const file of walk(API_DIR, [".ts", ".tsx"])) {
    const text = readFileSync(file, "utf8");
    const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
    const visit = (node) => {
      if (
        ts.isCallExpression(node) &&
        ts.isIdentifier(node.expression) &&
        node.expression.text === "invoke"
      ) {
        const [nameArg, argsArg] = node.arguments;
        const name = nameArg && ts.isStringLiteralLike(nameArg) ? nameArg.text : null;
        const keys = [];
        if (argsArg && ts.isObjectLiteralExpression(argsArg)) {
          for (const property of argsArg.properties) {
            if (ts.isPropertyAssignment(property)) {
              if (ts.isIdentifier(property.name) || ts.isStringLiteralLike(property.name)) {
                keys.push(property.name.text);
              } else {
                keys.push("<computed>");
              }
            } else if (ts.isShorthandPropertyAssignment(property)) {
              keys.push(property.name.text);
            } else {
              keys.push("<spread>");
            }
          }
        }
        const { line } = source.getLineAndCharacterOfPosition(node.getStart(source));
        calls.push({
          name,
          keys: argsArg && ts.isObjectLiteralExpression(argsArg) ? keys : null,
          file: relative(root, file),
          line: line + 1,
        });
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  }
  return calls;
}

/** Tauri maps JavaScript camelCase argument names to Rust snake_case parameters. */
function toSnakeCase(key) {
  return key.replace(/[A-Z]/g, (char) => `_${char.toLowerCase()}`);
}

function main() {
  const handlers = readHandlerList();
  const rustCommands = readRustCommands();
  const invocations = readInvocations();

  const errors = [];
  const warnings = [];
  const invoked = new Map();

  for (const name of handlers) {
    if (!rustCommands.has(name)) {
      errors.push(
        `generate_handler! lists \`${name}\`, but no #[tauri::command] fn ${name} exists in src-tauri/src`,
      );
    }
  }
  for (const [name, command] of rustCommands) {
    if (!handlers.includes(name)) {
      warnings.push(
        `#[tauri::command] ${name} (${relative(root, command.file)}:${command.line}) is not in generate_handler![], so the webview cannot call it`,
      );
    }
  }

  for (const call of invocations) {
    if (call.name === null) {
      warnings.push(`${call.file}:${call.line} calls invoke() with a non-literal command name; skipped`);
      continue;
    }
    if (!handlers.includes(call.name)) {
      errors.push(
        `${call.file}:${call.line} invokes \`${call.name}\`, which is not registered in generate_handler![]`,
      );
      continue;
    }
    invoked.set(call.name, true);
    const command = rustCommands.get(call.name);
    if (!command || !call.keys) continue;
    const expected = new Set(command.params);
    for (const key of call.keys) {
      if (key.startsWith("<")) {
        errors.push(
          `${call.file}:${call.line} passes an unsupported argument object member (${key}) to \`${call.name}\`; use a plain literal like the other bindings`,
        );
        continue;
      }
      if (!expected.has(toSnakeCase(key))) {
        errors.push(
          `${call.file}:${call.line} invokes \`${call.name}\` with argument \`${key}\`, but its Rust parameters are [${command.params.join(", ")}]`,
        );
      }
    }
    for (const param of expected) {
      const camel = param.replace(/_([a-z0-9])/g, (_, char) => char.toUpperCase());
      if (!call.keys.includes(param) && !call.keys.includes(camel)) {
        errors.push(
          `${call.file}:${call.line} invokes \`${call.name}\` without required argument \`${camel}\` (Rust parameter \`${param}\`)`,
        );
      }
    }
  }

  for (const name of handlers) {
    if (!invoked.has(name)) {
      warnings.push(`handler \`${name}\` is registered but not invoked from src/lib/api`);
    }
  }

  for (const warning of warnings) console.warn(`warning: ${warning}`);
  for (const error of errors) console.error(`error: ${error}`);

  if (errors.length > 0) {
    console.error(
      `\nIPC contract FAILED: ${errors.length} error(s), ${warnings.length} warning(s) across ${handlers.length} handlers and ${invocations.length} frontend invocations`,
    );
    process.exitCode = 1;
    return;
  }
  console.log(
    `IPC contract OK: ${handlers.length} handlers, ${invocations.length} frontend invocations, ${warnings.length} warning(s)`,
  );
}

try {
  main();
} catch (error) {
  if (process.env.IPC_DEBUG) console.error(error.stack);
  console.error(`IPC contract check could not run: ${error.message}`);
  process.exitCode = 2;
}

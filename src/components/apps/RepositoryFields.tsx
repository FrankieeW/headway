import { useTranslation } from "react-i18next";
import { Field } from "./FormField";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import type { RepositoryOption } from "@/lib/api";

export function RepositoryFields({
  rows,
  onChange,
}: {
  readonly rows: RepositoryOption[];
  readonly onChange: (rows: RepositoryOption[]) => void;
}) {
  const { t } = useTranslation();
  const change = (index: number, patch: Partial<RepositoryOption>) =>
    onChange(rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));
  return (
    <section className="space-y-3 border-t border-border pt-4">
      <h3 className="text-body font-medium">{t("versions.repositories")}</h3>
      <p className="text-caption text-fg-3">{t("versions.repositoriesHint")}</p>
      {rows.map((row, index) => (
        <div key={index} className="space-y-3 rounded-control bg-subtle p-3">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field label={t("versions.repositoryLabel")}>
              <Input
                aria-label={`${t("versions.repositoryLabel")} ${index + 1}`}
                value={row.label}
                onChange={(event) =>
                  change(index, { label: event.target.value })
                }
              />
            </Field>
            <Field label={t("form.repository")}>
              <Input
                aria-label={`${t("form.repository")} ${index + 1}`}
                value={row.repo}
                onChange={(event) =>
                  change(index, { repo: event.target.value })
                }
                placeholder="owner/repo"
              />
            </Field>
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <Field label={t("config.gitUrl")}>
              <Input
                aria-label={`${t("config.gitUrl")} ${index + 1}`}
                value={row.git_url ?? ""}
                onChange={(event) =>
                  change(index, { git_url: event.target.value })
                }
                className="font-mono"
              />
            </Field>
            <Field label={t("form.branch")}>
              <Input
                aria-label={`${t("form.branch")} ${index + 1}`}
                value={row.branch ?? ""}
                onChange={(event) =>
                  change(index, { branch: event.target.value })
                }
              />
            </Field>
          </div>
          <Button
            variant="neutral"
            onClick={() => onChange(rows.filter((_, i) => i !== index))}
          >
            {t("versions.removeRepository", { index: index + 1 })}
          </Button>
        </div>
      ))}
      <Button
        variant="neutral"
        onClick={() => onChange([...rows, { repo: "", label: "" }])}
      >
        {t("versions.addRepository")}
      </Button>
    </section>
  );
}

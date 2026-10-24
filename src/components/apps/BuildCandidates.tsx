import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { BuildCandidate } from "@/lib/api";

export function BuildCandidates({
  candidates,
  onApply,
}: {
  readonly candidates: BuildCandidate[];
  readonly onApply: (candidate: BuildCandidate) => void;
}) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState("");
  const candidate =
    candidates.find((entry) => entry.id === selected) ??
    candidates.find((entry) => entry.recommended) ??
    candidates[0];
  if (!candidate) return null;
  return (
    <section className="space-y-2 rounded-control bg-subtle p-3">
      <label className="text-caption font-medium" id="build-candidates-label">
        {t("roadmap.candidates")}
      </label>
      <Select value={candidate.id} onValueChange={setSelected}>
        <SelectTrigger aria-labelledby="build-candidates-label">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {candidates.map((entry) => (
            <SelectItem key={entry.id} value={entry.id}>
              {t(`roadmap.sources.${entry.source}`)} · {entry.file}
              {entry.line ? `:${entry.line}` : ""}
              {entry.recommended ? ` · ${t("roadmap.recommended")}` : ""}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p className="break-all text-caption text-fg-2">
        {candidate.file}
        {candidate.line ? `:${candidate.line}` : ""} · {candidate.reason}
      </p>
      <pre className="whitespace-pre-wrap break-all font-mono text-caption">
        {candidate.build}
      </pre>
      <p className="break-all text-caption text-fg-3">
        {t("form.workdir")}: {candidate.workdir || "."} · {t("form.artifact")}:{" "}
        {candidate.artifact || "—"}
      </p>
      <Button variant="neutral" onClick={() => onApply(candidate)}>
        {t("roadmap.applyCandidate")}
      </Button>
    </section>
  );
}

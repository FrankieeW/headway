import { expect, it } from "vitest";
import type { AppView } from "@/lib/api";
import { handleRoadmap } from "./mockRoadmap";

it("registers Headway first with notify-only defaults and preserves user overrides on repeated startup", () => {
  // Given an existing third-party subscription.
  const apps: AppView[] = [
    {
      app: {
        source: "npm",
        package: "tool",
        dist_tag: "latest",
        id: "tool",
        name: "Tool",
        kind: "cli",
        auto_update: false,
      },
      busy: false,
      status: "unchecked",
      pending_count: 0,
    },
  ];
  handleRoadmap("ensure_self_subscription", undefined, apps);
  const self = apps[0];
  if (!self) throw new Error("Self subscription missing");
  expect(self.app.id).toBe("headway");
  expect(self.app.auto_update).toBe(false);
  expect(self.app.update_policy?.mode).toBe("notify");
  self.app = {
    ...self.app,
    name: "My Headway",
    auto_update: true,
    update_policy: { mode: "next", max_steps: 2, pause_on_failure: true },
  };
  apps.reverse();
  // When startup runs again after a user customized and reordered the subscription.
  handleRoadmap("ensure_self_subscription", undefined, apps);
  // Then the existing self subscription moves first without duplicates or lost choices.
  expect(apps).toHaveLength(2);
  expect(apps[0]?.app).toMatchObject({
    id: "headway",
    name: "My Headway",
    auto_update: true,
    update_policy: { mode: "next", max_steps: 2 },
  });
});

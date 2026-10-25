import type { SelfUpdateStatus } from "@/lib/api";

/**
 * How often the self-update card polls while a restart is in flight.
 *
 * `self_update_status` runs `PlistBuddy` twice and the installed `headway`
 * binary once per call, so it must never poll on a fixed short interval while
 * idle. When nothing is pending the card relies on mount, window focus and an
 * explicit refresh instead.
 */
export const SELF_UPDATE_ACTIVE_POLL_MS = 5_000;

/**
 * Whether the backend is in the middle of a restart handshake. A terminal
 * `restart_error` is excluded: it only changes when the user retries, so
 * polling it would spawn subprocesses without ever changing the state.
 */
export function selfUpdateIsActive(status: SelfUpdateStatus | undefined): boolean {
  if (!status || status.restart_error) {
    return false;
  }
  return Boolean(status.pending_restart || status.restart_requested);
}

/**
 * `refetchInterval` for the self-update query: poll only while a restart is
 * pending/queued (the state changes on its own then), otherwise stop entirely.
 */
export function selfUpdateRefetchInterval(
  status: SelfUpdateStatus | undefined,
): number | false {
  return selfUpdateIsActive(status) ? SELF_UPDATE_ACTIVE_POLL_MS : false;
}

// Subscription plan monitoring.
//
// Why this exists: the deviceSecret never expires, but the MANGA Plus
// server's idea of the account's plan does. The entitlement is backed
// by a Google Play receipt that only the official app can refresh
// (subscription_restore requires purchase_data + a Google signature),
// and when the attested billing period ends without a refresh the
// server silently downgrades the account to "basic". Observed live on
// 2026-09-04: a deluxe account showed plan_type "basic" and DELUXE
// chapters started refusing with "Invalid user access(11301)".
//
// The desktop app can't refresh the receipt itself, but it CAN detect
// the drop early: poll GET /api/subscription (throttled, via the home
// page), remember the best plan ever seen, and surface a banner when
// the current plan falls below it — telling the user it's time to open
// the official app in the rooted emulator and tap Restore.
//
// Pure decision logic lives here (unit-tested); localStorage helpers
// follow the same conventions as readState.ts.

export type PlanWarning =
  | { kind: 'dropped'; from: string; to: string }
  | { kind: 'lapsed'; plan: string; nextPaymentDate: number };

/** Order plans by entitlement. Unknown/empty strings rank as basic —
 *  the server sometimes omits plan_type entirely (settings_v2 does). */
export function planRank(plan: string | null | undefined): number {
  switch ((plan ?? '').toLowerCase()) {
    case 'deluxe': return 2;
    case 'standard': return 1;
    default: return 0; // '', 'basic', anything unrecognized
  }
}

// A paid plan whose billing period ended this long ago without rolling
// forward is considered lapsed. Renewals normally reflect within hours;
// three days filters out server-side processing delays.
export const LAPSE_GRACE_SECS = 3 * 24 * 3600;

/** Decide whether the current subscription state deserves a warning.
 *
 *  - `dropped`: the plan is lower than the best plan this install has
 *    seen — the classic silent downgrade. Fires regardless of dates.
 *  - `lapsed`: still on a paid plan, but the attested period ended
 *    more than LAPSE_GRACE_SECS ago and never rolled forward — the
 *    renewal isn't reaching the server, a drop is likely imminent.
 *  - null: nothing to worry about (also for fresh installs with no
 *    remembered plan — there's nothing to compare against).
 */
export function subscriptionWarning(
  currentPlan: string,
  bestSeenPlan: string | null,
  nextPaymentDate: number,
  nowSecs: number,
): PlanWarning | null {
  if (bestSeenPlan != null && planRank(currentPlan) < planRank(bestSeenPlan)) {
    return { kind: 'dropped', from: bestSeenPlan, to: currentPlan || 'basic' };
  }
  if (
    planRank(currentPlan) > 0 &&
    nextPaymentDate > 0 &&
    nowSecs > nextPaymentDate + LAPSE_GRACE_SECS
  ) {
    return { kind: 'lapsed', plan: currentPlan, nextPaymentDate };
  }
  return null;
}

// ---------- persistence (localStorage, mirrors readState.ts style) ----------

const KEY_BEST_PLAN = 'mp:sub:bestPlan';
const KEY_LAST_CHECK = 'mp:sub:lastCheckSecs';
const KEY_DISMISSED = 'mp:sub:dismissedWarning';

// Poll at most twice a day. The plan changes on billing-cycle
// timescales; anything more frequent is wasted API traffic.
export const CHECK_INTERVAL_SECS = 12 * 3600;

export function getBestSeenPlan(): string | null {
  try {
    return localStorage.getItem(KEY_BEST_PLAN);
  } catch {
    return null;
  }
}

/** Remember the highest plan this install has observed. Never lowers —
 *  a downgrade must keep tripping the warning until the user either
 *  restores or dismisses it. */
export function rememberPlan(plan: string) {
  try {
    if (planRank(plan) > planRank(getBestSeenPlan())) {
      localStorage.setItem(KEY_BEST_PLAN, plan);
    }
  } catch (e) {
    console.warn('rememberPlan failed', e);
  }
}

export function shouldCheckNow(nowSecs: number): boolean {
  try {
    const raw = localStorage.getItem(KEY_LAST_CHECK);
    const last = raw ? parseInt(raw, 10) : 0;
    return !Number.isFinite(last) || nowSecs - last >= CHECK_INTERVAL_SECS;
  } catch {
    return true;
  }
}

export function markCheckedNow(nowSecs: number) {
  try {
    localStorage.setItem(KEY_LAST_CHECK, String(nowSecs));
  } catch (e) {
    console.warn('markCheckedNow failed', e);
  }
}

/** Dismissal is keyed on the warning's content: dismissing "dropped to
 *  basic" stays dismissed for that exact situation, but a later,
 *  different warning (or the same drop after an intermediate restore)
 *  surfaces again. */
export function isWarningDismissed(w: PlanWarning): boolean {
  try {
    return localStorage.getItem(KEY_DISMISSED) === JSON.stringify(w);
  } catch {
    return false;
  }
}

export function dismissWarning(w: PlanWarning) {
  try {
    localStorage.setItem(KEY_DISMISSED, JSON.stringify(w));
  } catch (e) {
    console.warn('dismissWarning failed', e);
  }
}

/** A successful check with no warning clears any stale dismissal so
 *  the next genuine problem isn't accidentally suppressed. */
export function clearDismissedWarning() {
  try {
    localStorage.removeItem(KEY_DISMISSED);
  } catch {
    /* ignore */
  }
}

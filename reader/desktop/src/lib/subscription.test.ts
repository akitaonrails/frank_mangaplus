import { describe, it, expect } from 'vitest';
import {
  planRank,
  subscriptionWarning,
  LAPSE_GRACE_SECS,
} from './subscription';

const NOW = 1_788_600_000; // ~2026-09-05

describe('planRank', () => {
  it('orders the three tiers', () => {
    expect(planRank('deluxe')).toBe(2);
    expect(planRank('standard')).toBe(1);
    expect(planRank('basic')).toBe(0);
  });

  it('treats unknown/absent values as basic', () => {
    expect(planRank('')).toBe(0);
    expect(planRank(null)).toBe(0);
    expect(planRank(undefined)).toBe(0);
    expect(planRank('ultra-mega')).toBe(0);
  });

  it('is case-insensitive', () => {
    expect(planRank('Deluxe')).toBe(2);
    expect(planRank('STANDARD')).toBe(1);
  });
});

describe('subscriptionWarning', () => {
  const future = NOW + 30 * 24 * 3600;

  it('warns when the plan drops below the best seen', () => {
    // The live-observed silent downgrade: deluxe → basic.
    expect(subscriptionWarning('basic', 'deluxe', future, NOW)).toEqual({
      kind: 'dropped',
      from: 'deluxe',
      to: 'basic',
    });
    expect(subscriptionWarning('standard', 'deluxe', future, NOW)).toEqual({
      kind: 'dropped',
      from: 'deluxe',
      to: 'standard',
    });
  });

  it('normalizes an empty current plan to "basic" in the warning', () => {
    // settings_v2 omits plan_type for basic accounts; the warning text
    // should still read sensibly.
    expect(subscriptionWarning('', 'deluxe', 0, NOW)).toEqual({
      kind: 'dropped',
      from: 'deluxe',
      to: 'basic',
    });
  });

  it('stays quiet when the plan matches or improves', () => {
    expect(subscriptionWarning('deluxe', 'deluxe', future, NOW)).toBe(null);
    expect(subscriptionWarning('deluxe', 'standard', future, NOW)).toBe(null);
    expect(subscriptionWarning('deluxe', null, future, NOW)).toBe(null);
  });

  it('stays quiet for fresh installs with nothing remembered', () => {
    expect(subscriptionWarning('basic', null, 0, NOW)).toBe(null);
  });

  it('warns lapsed when a paid period ended beyond the grace window', () => {
    const ended = NOW - LAPSE_GRACE_SECS - 60;
    expect(subscriptionWarning('deluxe', 'deluxe', ended, NOW)).toEqual({
      kind: 'lapsed',
      plan: 'deluxe',
      nextPaymentDate: ended,
    });
  });

  it('gives renewals the grace window before calling it lapsed', () => {
    const endedRecently = NOW - LAPSE_GRACE_SECS + 3600;
    expect(subscriptionWarning('deluxe', 'deluxe', endedRecently, NOW)).toBe(null);
  });

  it('never calls a basic plan lapsed', () => {
    // basic has nothing to lapse — and the live-observed basic record
    // even carried a (stale) future next_payment_date.
    expect(subscriptionWarning('basic', 'basic', NOW - 10 * 24 * 3600, NOW)).toBe(null);
  });

  it('ignores a zero next_payment_date', () => {
    expect(subscriptionWarning('deluxe', 'deluxe', 0, NOW)).toBe(null);
  });
});

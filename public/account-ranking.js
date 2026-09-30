function getRemainingQuota(window) {
  if (typeof window?.usedPercent !== 'number' || !Number.isFinite(window.usedPercent)) return null;
  return Math.max(0, Math.min(100, 100 - window.usedPercent));
}

function getAccountRank(account) {
  const usage = account.usage;
  const five = getRemainingQuota(usage?.primary);
  const weekly = getRemainingQuota(usage?.secondary);
  const blocked = usage?.limitReached === true || five === 0 || weekly === 0;
  // Known usable quotas outrank unknown data; exhausted quotas and errors come last.
  const group = usage?.error ? 3 : blocked ? 2 : five === null || weekly === null ? 1 : 0;
  const exhausted = [usage?.primary, usage?.secondary].filter(window => getRemainingQuota(window) === 0);
  const resetWait = blocked && exhausted.length
    ? Math.max(...exhausted.map(window => typeof window.resetAfterSeconds === 'number' && window.resetAfterSeconds > 0
      ? window.resetAfterSeconds : Infinity))
    : Infinity;
  return {
    group,
    bottleneck: Math.min(five ?? 0, weekly ?? 0),
    five: five ?? 0,
    weekly: weekly ?? 0,
    resetWait,
    credits: Number.isFinite(usage?.resetCredits) ? Math.max(0, usage.resetCredits) : 0,
  };
}

function rankAccounts(accounts) {
  return [...accounts].sort((a, b) => {
    const left = getAccountRank(a);
    const right = getAccountRank(b);
    if (left.group !== right.group) return left.group - right.group;
    if (left.group === 2 && left.resetWait !== right.resetWait) return left.resetWait < right.resetWait ? -1 : 1;
    return right.bottleneck - left.bottleneck
      || right.five - left.five
      || right.weekly - left.weekly
      || right.credits - left.credits
      || Number(!!b.isActive) - Number(!!a.isActive)
      || String(a.email || a.id || '').localeCompare(String(b.email || b.id || ''))
      || String(a.id || '').localeCompare(String(b.id || ''));
  });
}

if (typeof module !== 'undefined') module.exports = { rankAccounts, getRemainingQuota, getAccountRank };

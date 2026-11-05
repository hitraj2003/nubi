// Shared usage copy. Cached values keep the time of their last successful update.
window.UsageStatus = {
  age(usage, now = Date.now()) {
    if (!usage?.fetched_ms || !usage.windows?.length) return "Not updated yet";
    const minutes = Math.max(0, Math.floor((now - usage.fetched_ms) / 60000));
    return minutes < 1 ? "Updated just now" : `Updated ${minutes} min ago`;
  },
  // Stale: no data, or older than 15 minutes (usage refreshes every few minutes).
  stale(usage, now = Date.now()) {
    return !usage?.windows?.length || !usage.fetched_ms || now - usage.fetched_ms > 15 * 60000;
  },
  text(usage, now = Date.now()) {
    if (usage?.windows?.length) return this.age(usage, now);
    return {
      loading: "Loading usage...", needs_auth: "Sign in again", no_auth: "Sign in to see usage",
      backoff: "Waiting to try again", error: "Could not update usage",
    }[usage?.status] || "Usage unavailable";
  },
};

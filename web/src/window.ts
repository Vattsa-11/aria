export class SlidingWindow {
  private hits: number[] = [];

  hit(limit: number, windowMs: number, now = Date.now()): { allowed: boolean; retryAfter: number } {
    this.hits = this.hits.filter((time) => now - time < windowMs);
    if (this.hits.length >= limit) {
      return { allowed: false, retryAfter: Math.max(1, Math.ceil((this.hits[0] + windowMs - now) / 1000)) };
    }
    this.hits.push(now);
    return { allowed: true, retryAfter: 0 };
  }
}

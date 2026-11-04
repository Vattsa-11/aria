import { DurableObject } from "cloudflare:workers";
import { SlidingWindow } from "./window.ts";

export class RateLimiter extends DurableObject {
  private window = new SlidingWindow();

  async hit(limit: number, windowMs: number): Promise<{ allowed: boolean; retryAfter: number }> {
    return this.window.hit(limit, windowMs);
  }
}

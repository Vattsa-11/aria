import { DurableObject } from "cloudflare:workers";

interface Entry {
  value: string;
  until: number;
}

export class EdgeStore extends DurableObject {
  async get(key: string): Promise<string | null> {
    const entry = await this.ctx.storage.get<Entry>(key);
    if (!entry) return null;
    if (entry.until <= Date.now()) {
      await this.ctx.storage.delete(key);
      return null;
    }
    return entry.value;
  }

  async set(key: string, value: string, seconds: number): Promise<void> {
    await this.ctx.storage.put<Entry>(key, { value, until: Date.now() + seconds * 1000 });
  }
}

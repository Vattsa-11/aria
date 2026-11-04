export interface Namespace {
  idFromName(name: string): unknown;
  get(id: unknown): { hit(limit: number, windowMs: number): Promise<{ allowed: boolean; retryAfter: number }> };
}

export const POLICY = {
  search: { limit: 30, windowMs: 60_000 },
  audio: { limit: 300, windowMs: 60_000 },
};

export async function retryAfter(namespace: Namespace | undefined, request: Request, pathname: string): Promise<number> {
  if (!namespace) return 0;
  const kind = pathname === "/api/audio" ? "audio" : "search";
  const client = request.headers.get("cf-connecting-ip") ?? "unknown";
  try {
    const result = await namespace.get(namespace.idFromName(`${client}:${kind}`)).hit(POLICY[kind].limit, POLICY[kind].windowMs);
    return result.allowed ? 0 : result.retryAfter;
  } catch {
    return 0;
  }
}

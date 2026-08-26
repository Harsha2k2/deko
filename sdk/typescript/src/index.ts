export class Deko {
  constructor(private config: { baseUrl?: string; apiKey?: string; jwt?: string } = {}) {}
  async check(intent: string, opts: any = {}) {
    // thin wrapper — delegates to fetch /action?wait=true
    const base = this.config.baseUrl ?? process.env.DEKO_URL ?? "http://localhost:8000";
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (this.config.jwt) headers["Authorization"] = `Bearer ${this.config.jwt}`;
    else if (this.config.apiKey) headers["X-API-Key"] = this.config.apiKey;
    const res = await fetch(`${base}/action?wait=true&timeout=30`, {
      method: "POST",
      headers,
      body: JSON.stringify({ intent, ...opts }),
    });
    if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);
    return res.json();
  }
  guard<T extends (...args: any[]) => any>(fn: T, opts: any = {}): T {
    const deko = this;
    return (async (...args: any[]) => {
      const intent = fn.name + "(" + JSON.stringify(args) + ")";
      const v = await deko.check(intent, opts);
      // @ts-ignore
      if (v.verdict?.decision === "denied") throw new Error(`deko denied: ${v.verdict.reason}`);
      if (v.verdict?.decision === "escalate") throw new Error(`deko escalated: ${v.verdict.reason}`);
      return (fn as any)(...args);
    }) as T;
  }
}
export const dekoMiddleware = () => (opts: any) => opts;

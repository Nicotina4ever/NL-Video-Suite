export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export async function api<T = unknown>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: init?.body ? { 'Content-Type': 'application/json', ...init.headers } : init?.headers,
  });
  const value = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new ApiError(response.status, (value as { error?: string }).error || `${response.status} ${path}`);
  }
  return value as T;
}

export const get = <T>(path: string) => api<T>(path);
export const post = <T>(path: string, body?: unknown) =>
  api<T>(path, { method: 'POST', body: body === undefined ? undefined : JSON.stringify(body) });
export const put = <T>(path: string, body: unknown) => api<T>(path, { method: 'PUT', body: JSON.stringify(body) });

export interface ProjectSummary {
  id: string;
  title: string;
  updated_at: string;
  assets: { id: string; name: string; kind: string }[];
}

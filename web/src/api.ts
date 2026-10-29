export class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export async function request<T>(
  token: string,
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    cache: "no-store",
    credentials: "omit",
    headers: {
      ...(init.body
        ? {
            "Content-Type":
              init.body instanceof Blob
                ? "application/zip"
                : "application/json",
          }
        : {}),
      Authorization: `Bearer ${token}`,
    },
  });
  const data = await response.json().catch(() => null);
  if (!response.ok) {
    throw new ApiError(
      response.status,
      data?.error || `请求失败（${response.status}），请检查输入或服务状态。`,
    );
  }
  return data as T;
}

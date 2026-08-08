import { http } from "@/lib/http";

export type LoginRequest = {
  email: string;
  password: string;
};

export type RegisterRequest = {
  username: string;
  email: string;
  password: string;
};

export const authApi = http;

export async function login(
  body: LoginRequest,
) {
  const response = await authApi.post(
    "/auth/login",
    body,
  );

  return response.data;
}

export async function register(
  body: RegisterRequest,
) {
  const response = await authApi.post(
    "/auth/register",
    body,
  );

  return response.data;
}

export async function logout() {
  const response = await authApi.post(
    "/auth/logout",
  );

  return response.data;
}

export async function refreshToken(
  refreshToken: string,
) {
  const response = await authApi.post(
    "/auth/refresh",
    {
      refresh_token: refreshToken,
    },
  );

  return response.data;
}

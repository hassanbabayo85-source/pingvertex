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

export type AuthResponse = {
  access_token: string;
};

export const authApi = http;

export async function login(
  body: LoginRequest,
): Promise<AuthResponse> {
  const response = await authApi.post(
    "/auth/login",
    body,
  );

  return response.data;
}

export async function register(
  body: RegisterRequest,
): Promise<AuthResponse> {
  const response = await authApi.post(
    "/auth/register",
    body,
  );

  return response.data;
}
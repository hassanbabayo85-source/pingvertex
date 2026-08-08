export type User = {
  id: string;
  email: string;
  username: string;
  avatar_url: string | null;
};

export type LoginRequest = {
  email: string;
  password: string;
};

export type RegisterRequest = {
  username: string;
  email: string;
  password: string;
};

export type AuthTokens = {
  access_token: string;
  refresh_token: string;
};

export type AuthSession = {
  user: User;
  tokens: AuthTokens;
};

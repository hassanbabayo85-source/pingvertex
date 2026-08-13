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

export type AuthResponse = {
  access_token: string;
};

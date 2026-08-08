import axios from "axios";

import {
  getAccessToken,
  clearTokens,
} from "@/lib/auth";

export const http = axios.create({
  baseURL:
    process.env.NEXT_PUBLIC_API_URL ??
    "http://localhost:8080/api",
  headers: {
    "Content-Type": "application/json",
  },
});

http.interceptors.request.use((config) => {
  const token = getAccessToken();

  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }

  return config;
});

http.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      clearTokens();
    }

    return Promise.reject(error);
  },
);

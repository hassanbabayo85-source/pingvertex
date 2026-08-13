import axios from "axios";

import { clearTokens } from "@/lib/auth";

export const http = axios.create({
  baseURL:
    process.env.NEXT_PUBLIC_API_URL ??
    "http://localhost:8080/api",
  headers: {
    "Content-Type": "application/json",
  },
  // Access token yanzu httpOnly cookie ne (ba JavaScript-readable localStorage
  // ba), don haka muna bukatar wannan domin browser ya aika/karba cookies a
  // kowace kira zuwa API.
  withCredentials: true,
});

// Ba mu kara amfani da interceptor da ke kara Authorization header daga
// localStorage — cookie zata tafi ta atomatik godiya ga `withCredentials: true`.

http.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      // Ba mu da token da za mu share a nan — cookie ta server ce ke
      // sarrafa ta. Mun share flag ɗin UI kawai.
      clearTokens();
    }

    return Promise.reject(error);
  },
);

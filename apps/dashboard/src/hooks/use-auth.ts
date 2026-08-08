"use client";

import { getAccessToken } from "@/lib/auth";

export function useAuth() {
  const authenticated = !!getAccessToken();

  return {
    authenticated,
  };
}

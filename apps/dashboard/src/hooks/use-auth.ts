"use client";

import { isMarkedAuthenticated } from "@/lib/auth";

export function useAuth() {
  const authenticated = isMarkedAuthenticated();

  return {
    authenticated,
  };
}

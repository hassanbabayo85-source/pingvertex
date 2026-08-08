"use client";

import { useMutation } from "@tanstack/react-query";
import { login } from "@/lib/auth-api";

export function useLogin() {
  return useMutation({
    mutationFn: login,
  });
}

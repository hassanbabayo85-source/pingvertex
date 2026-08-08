"use client";

import axios from "axios";
import { useMutation } from "@tanstack/react-query";
import { useRouter } from "next/navigation";
import { toast } from "sonner";

import { createMonitor, type CreateMonitorPayload } from "@/lib/api";

function extractErrorMessage(error: unknown) {
  if (axios.isAxiosError(error)) {
    const data = error.response?.data;

    if (typeof data === "string") {
      return data;
    }

    if (data && typeof data === "object" && "message" in data) {
      const message = (data as { message?: unknown }).message;
      if (typeof message === "string") {
        return message;
      }
    }

    return error.message;
  }

  if (error instanceof Error) {
    return error.message;
  }

  return "Failed to create monitor";
}

export function useCreateMonitor(onSuccess?: () => void) {
  const router = useRouter();

  return useMutation({
    mutationFn: (payload: CreateMonitorPayload) => createMonitor(payload),
    onSuccess: () => {
      toast.success("Monitor created successfully");
      onSuccess?.();
      router.refresh();
    },
    onError: (error) => {
      toast.error(extractErrorMessage(error));
    },
  });
}

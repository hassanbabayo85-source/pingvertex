"use client";

import axios from "axios";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";

import { deleteMonitor } from "@/lib/api";

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

  return "Failed to delete monitor";
}

export function useDeleteMonitor(onSuccess?: () => void) {
  return useMutation({
    mutationFn: (id: string) => deleteMonitor(id),
    onSuccess: () => {
      toast.success("Monitor deleted successfully");
      onSuccess?.();
    },
    onError: (error) => {
      toast.error(extractErrorMessage(error));
    },
  });
}

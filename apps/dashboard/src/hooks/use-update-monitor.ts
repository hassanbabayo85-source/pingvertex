"use client";

import axios from "axios";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";

import { updateMonitor, type CreateMonitorPayload } from "@/lib/api";

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

  return "Failed to update monitor";
}

type UpdateMonitorInput = {
  id: string;
  payload: CreateMonitorPayload;
};

export function useUpdateMonitor(onSuccess?: () => void) {
  return useMutation({
    mutationFn: ({ id, payload }: UpdateMonitorInput) =>
      updateMonitor(id, payload),
    onSuccess: () => {
      toast.success("Monitor updated successfully");
      onSuccess?.();
    },
    onError: (error) => {
      toast.error(extractErrorMessage(error));
    },
  });
}

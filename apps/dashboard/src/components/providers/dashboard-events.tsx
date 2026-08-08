"use client";

import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { connectDashboardEvents } from "@/lib/events";

export function DashboardEvents() {
  const queryClient = useQueryClient();

  useEffect(() => {
    return connectDashboardEvents((event) => {
      queryClient.invalidateQueries();

      switch (event.type) {
        case "monitor.updated":
          toast.info("Monitor updated");
          break;

        case "check.completed":
          toast.success("Health check completed");
          break;

        case "incident.created":
          toast.error("New incident detected");
          break;

        case "incident.resolved":
          toast.success("Incident resolved");
          break;

        default:
          break;
      }
    });
  }, [queryClient]);

  return null;
}

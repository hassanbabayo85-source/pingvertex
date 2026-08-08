"use client";

import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Gauge,
} from "lucide-react";

import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";

type Monitor = {
  id: string;
  status?: "Up" | "Down" | "Idle";
  latency_ms?: number | null;
};

type Props = {
  monitors: Monitor[];
};

export function DashboardSummary({ monitors }: Props) {
  const total = monitors.length;

  const up = monitors.filter((m) => m.status === "Up").length;
  const down = monitors.filter((m) => m.status === "Down").length;

  const latency = monitors
    .map((m) => m.latency_ms)
    .filter((v): v is number => typeof v === "number");

  const avg =
    latency.length === 0
      ? 0
      : Math.round(latency.reduce((a, b) => a + b, 0) / latency.length);

  const cards = [
    {
      title: "Total Monitors",
      value: total,
      icon: Activity,
      color: "text-blue-600",
      bg: "bg-blue-50 dark:bg-blue-950/20",
    },
    {
      title: "Healthy",
      value: up,
      icon: CheckCircle2,
      color: "text-emerald-600",
      bg: "bg-emerald-50 dark:bg-emerald-950/20",
    },
    {
      title: "Down",
      value: down,
      icon: AlertTriangle,
      color: "text-rose-600",
      bg: "bg-rose-50 dark:bg-rose-950/20",
    },
    {
      title: "Avg Latency",
      value: `${avg} ms`,
      icon: Gauge,
      color: "text-amber-600",
      bg: "bg-amber-50 dark:bg-amber-950/20",
    },
  ];

  return (
    <div className="grid gap-5 md:grid-cols-2 xl:grid-cols-4">
      {cards.map((card) => {
        const Icon = card.icon;

        return (
          <Card
            key={card.title}
            className="transition-all duration-200 hover:-translate-y-1 hover:shadow-lg"
          >
            <CardContent className="flex items-center justify-between p-6">
              <div>
                <p className="text-sm text-muted-foreground">{card.title}</p>
                <h2 className="mt-2 text-3xl font-bold">{card.value}</h2>
              </div>

              <div className={cn("rounded-xl p-3", card.bg)}>
                <Icon className={cn("h-7 w-7", card.color)} />
              </div>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}

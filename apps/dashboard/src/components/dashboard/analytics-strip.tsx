"use client";

import { Activity, Gauge, TrendingDown, TrendingUp } from "lucide-react";

import { Card, CardContent } from "@/components/ui/card";

type Props = {
  latencies: number[];
  openIncidents: number;
  monitorCount: number;
};

function average(values: number[]) {
  if (values.length === 0) return 0;
  return Math.round(values.reduce((a, b) => a + b, 0) / values.length);
}

export function AnalyticsStrip({
  latencies,
  openIncidents,
  monitorCount,
}: Props) {
  const avg = average(latencies);
  const min = latencies.length === 0 ? 0 : Math.min(...latencies);
  const max = latencies.length === 0 ? 0 : Math.max(...latencies);

  const items = [
    {
      label: "Monitors",
      value: monitorCount,
      icon: Activity,
    },
    {
      label: "Avg Latency",
      value: `${avg} ms`,
      icon: Gauge,
    },
    {
      label: "Fastest",
      value: `${min} ms`,
      icon: TrendingDown,
    },
    {
      label: "Slowest",
      value: `${max} ms`,
      icon: TrendingUp,
    },
    {
      label: "Open Incidents",
      value: openIncidents,
      icon: Activity,
    },
  ];

  return (
    <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-5">
      {items.map((item) => {
        const Icon = item.icon;

        return (
          <Card key={item.label} className="transition-all hover:shadow-md">
            <CardContent className="flex items-center justify-between p-5">
              <div>
                <p className="text-sm text-muted-foreground">{item.label}</p>
                <div className="mt-2 text-2xl font-bold">{item.value}</div>
              </div>
              <Icon className="h-5 w-5 text-muted-foreground" />
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}

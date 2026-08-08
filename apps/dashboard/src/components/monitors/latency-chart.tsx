"use client";

import {
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

type CheckPoint = {
  time: string;
  latency: number;
  status: "Up" | "Down";
};

type LatencyChartProps = {
  data: CheckPoint[];
};

function formatTime(value: string) {
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

export function LatencyChart({ data }: LatencyChartProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Response time</CardTitle>
      </CardHeader>
      <CardContent className="h-80">
        {data.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            No latency data yet.
          </div>
        ) : (
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={data}>
              <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
              <XAxis
                dataKey="time"
                tickFormatter={formatTime}
                tick={{ fontSize: 12 }}
              />
              <YAxis
                tick={{ fontSize: 12 }}
                width={50}
                tickFormatter={(value) => `${value} ms`}
              />
              <Tooltip
                labelFormatter={(value) => formatTime(String(value))}
                formatter={(value) => `${String(value)} ms`}
              />
              <Line
                type="monotone"
                dataKey="latency"
                stroke="currentColor"
                strokeWidth={2}
                dot={{ r: 3 }}
                activeDot={{ r: 6 }}
                className="text-primary"
              />
            </LineChart>
          </ResponsiveContainer>
        )}
      </CardContent>
    </Card>
  );
}

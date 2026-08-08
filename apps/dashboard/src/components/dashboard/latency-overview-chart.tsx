"use client";

import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

type ChartPoint = {
  name: string;
  latency: number;
};

type Props = {
  data: ChartPoint[];
};

export function LatencyOverviewChart({ data }: Props) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Latency overview</CardTitle>
      </CardHeader>
      <CardContent className="h-80">
        {data.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
            No latency data available.
          </div>
        ) : (
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={data}>
              <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
              <XAxis
                dataKey="name"
                tick={{ fontSize: 12 }}
                interval={0}
                angle={-20}
                textAnchor="end"
                height={60}
              />
              <YAxis
                tick={{ fontSize: 12 }}
                width={50}
                tickFormatter={(value) => `${value} ms`}
              />
              <Tooltip
                formatter={(value) => `${String(value)} ms`}
                labelStyle={{ fontWeight: 600 }}
              />
              <Bar dataKey="latency" radius={[8, 8, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        )}
      </CardContent>
    </Card>
  );
}

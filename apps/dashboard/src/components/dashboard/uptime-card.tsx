import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface Props {
  uptime: number;
}

export function UptimeCard({ uptime }: Props) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium">
          Uptime
        </CardTitle>
      </CardHeader>

      <CardContent>
        <div className="text-3xl font-bold">
          {uptime.toFixed(2)}%
        </div>

        <p className="mt-2 text-sm text-muted-foreground">
          Based on recent checks
        </p>
      </CardContent>
    </Card>
  );
}

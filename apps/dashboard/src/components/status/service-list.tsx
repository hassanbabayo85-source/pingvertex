import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { UptimeBadge } from "@/components/status/uptime-badge";

export type StatusService = {
  id: string;
  name: string;
  url: string;
  status: "Up" | "Down" | "Idle";
  latency: number | null;
  uptime: number;
};

type Props = {
  services: StatusService[];
};

export function ServiceList({ services }: Props) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Services</CardTitle>
      </CardHeader>

      <CardContent className="p-0">
        {services.length === 0 ? (
          <div className="p-6 text-sm text-muted-foreground">
            No services available.
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[800px]">
              <thead className="border-b bg-muted/40 text-left text-sm">
                <tr>
                  <th className="px-6 py-4 font-medium">Service</th>
                  <th className="px-6 py-4 font-medium">Status</th>
                  <th className="px-6 py-4 font-medium">Latency</th>
                  <th className="px-6 py-4 font-medium">Uptime</th>
                </tr>
              </thead>
              <tbody>
                {services.map((service) => (
                  <tr key={service.id} className="border-b last:border-b-0">
                    <td className="px-6 py-4">
                      <div className="font-medium">{service.name}</div>
                      <div className="text-sm text-muted-foreground">
                        {service.url}
                      </div>
                    </td>
                    <td className="px-6 py-4">
                      <UptimeBadge status={service.status} />
                    </td>
                    <td className="px-6 py-4 text-sm">
                      {service.latency === null ? "—" : `${service.latency} ms`}
                    </td>
                    <td className="px-6 py-4 text-sm font-medium">
                      {service.uptime.toFixed(2)}%
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

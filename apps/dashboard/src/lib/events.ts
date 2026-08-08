export type DashboardEvent =
  | {
      type: "monitor.updated";
      monitorId: string;
    }
  | {
      type: "check.completed";
      monitorId: string;
    }
  | {
      type: "incident.created";
      monitorId: string;
    }
  | {
      type: "incident.resolved";
      monitorId: string;
    };

export function connectDashboardEvents(
  onEvent: (event: DashboardEvent) => void,
) {
  const base =
    process.env.NEXT_PUBLIC_API_URL ?? "http://127.0.0.1:3000";

  const url = `${base}/events`;

  const source = new EventSource(url);

  source.onmessage = (message) => {
    try {
      onEvent(JSON.parse(message.data));
    } catch (err) {
      console.error(err);
    }
  };

  source.onerror = () => {
    console.warn("Event stream disconnected");
  };

  return () => source.close();
}

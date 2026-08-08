import { AuthProvider } from "@/providers/auth-provider";
import type { Metadata } from "next";
import "./globals.css";

import { QueryProvider } from "@/providers/query-provider";
import { ThemeProvider } from "@/providers/theme-provider";
import { DashboardEvents } from "@/components/providers/dashboard-events";
import { Toaster } from "sonner";

export const metadata: Metadata = {
  title: "Uptime Monitor",
  description: "Dashboard for monitors, checks, and incidents.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className="min-h-screen bg-background text-foreground antialiased">
        <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
          <QueryProvider>
          <DashboardEvents />
          <AuthProvider>{children}</AuthProvider>
        </QueryProvider>
        <Toaster richColors position="top-right" />
        </ThemeProvider>
      </body>
    </html>
  );
}

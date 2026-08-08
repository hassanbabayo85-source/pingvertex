"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";

import { useAuthContext } from "@/providers/auth-provider";

type ProtectedRouteProps = {
  children: React.ReactNode;
};

export function ProtectedRoute({
  children,
}: ProtectedRouteProps) {
  const router = useRouter();

  const {
    authenticated,
    loading,
  } = useAuthContext();

  useEffect(() => {
    if (!loading && !authenticated) {
      router.replace("/login");
    }
  }, [
    authenticated,
    loading,
    router,
  ]);

  if (loading) {
    return (
      <main className="flex min-h-screen items-center justify-center">
        <p className="text-muted-foreground">
          Loading session...
        </p>
      </main>
    );
  }

  if (!authenticated) {
    return null;
  }

  return <>{children}</>;
}

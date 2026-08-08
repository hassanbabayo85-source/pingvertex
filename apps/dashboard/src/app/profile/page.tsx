import { ProtectedRoute } from "@/components/auth/protected-route";

export default function ProfilePage() {
  return (
    <ProtectedRoute>
    <main className="flex min-h-screen items-center justify-center">
      <h1 className="text-3xl font-bold">
        Profile (Coming Next)
      </h1>
    </main>
      </ProtectedRoute>
  );
}

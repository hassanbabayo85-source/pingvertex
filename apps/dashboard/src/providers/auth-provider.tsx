"use client";

import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

import type { AuthSession } from "@/types/auth";
import {
  clearTokens,
  getAccessToken,
  getRefreshToken,
  saveTokens,
} from "@/lib/auth";

type AuthContextValue = {
  session: AuthSession | null;
  authenticated: boolean;
  loading: boolean;

  setSession: (session: AuthSession | null) => void;
  logout: () => void;
};

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({
  children,
}: {
  children: React.ReactNode;
}) {
  const [session, setSessionState] =
    useState<AuthSession | null>(null);

  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const access = getAccessToken();
    const refresh = getRefreshToken();

    if (access && refresh) {
      setSessionState({
        user: {
          id: "",
          email: "",
          username: "",
          avatar_url: null,
        },
        tokens: {
          access_token: access,
          refresh_token: refresh,
        },
      });
    }

    setLoading(false);
  }, []);

  function setSession(session: AuthSession | null) {
    if (!session) {
      clearTokens();
      setSessionState(null);
      return;
    }

    saveTokens(
      session.tokens.access_token,
      session.tokens.refresh_token,
    );

    setSessionState(session);
  }

  function logout() {
    clearTokens();
    setSessionState(null);
  }

  const value = useMemo<AuthContextValue>(
    () => ({
      session,
      authenticated: !!session,
      loading,
      setSession,
      logout,
    }),
    [session, loading],
  );

  return (
    <AuthContext.Provider value={value}>
      {children}
    </AuthContext.Provider>
  );
}

export function useAuthContext() {
  const ctx = useContext(AuthContext);

  if (!ctx) {
    throw new Error(
      "useAuthContext must be used inside AuthProvider",
    );
  }

  return ctx;
}

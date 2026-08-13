"use client";

import { useRouter } from "next/navigation";
import {
  LayoutDashboard,
  LogOut,
  Settings,
  User,
} from "lucide-react";

import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

import { useAuthContext } from "@/providers/auth-provider";

export function UserMenu() {
  const router = useRouter();
  const { logout } = useAuthContext();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger className="rounded-full outline-none">
        <Avatar className="h-10 w-10">
          <AvatarFallback>U</AvatarFallback>
        </Avatar>
      </DropdownMenuTrigger>

      <DropdownMenuContent align="end" className="w-60">
        <div className="px-3 py-2">
          <p className="font-medium">User</p>
          <p className="text-xs text-muted-foreground">
            Authenticated
          </p>
        </div>

        <DropdownMenuSeparator />

        <DropdownMenuItem onClick={() => router.push("/")}>
          <LayoutDashboard className="mr-2 h-4 w-4" />
          Dashboard
        </DropdownMenuItem>

        <DropdownMenuItem onClick={() => router.push("/profile")}>
          <User className="mr-2 h-4 w-4" />
          Profile
        </DropdownMenuItem>

        <DropdownMenuItem onClick={() => router.push("/settings")}>
          <Settings className="mr-2 h-4 w-4" />
          Settings
        </DropdownMenuItem>

        <DropdownMenuSeparator />

        <DropdownMenuItem
          onClick={() => {
            logout();
            router.replace("/login");
          }}
        >
          <LogOut className="mr-2 h-4 w-4" />
          Logout
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

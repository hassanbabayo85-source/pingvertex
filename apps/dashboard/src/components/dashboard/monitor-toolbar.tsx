"use client";

import { Search } from "lucide-react";

import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

type Props = {
  search: string;
  status: string;
  onSearchChange: (value: string) => void;
  onStatusChange: (value: string | null) => void;
};

export function MonitorToolbar({
  search,
  status,
  onSearchChange,
  onStatusChange,
}: Props) {
  return (
    <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
      <div className="relative w-full md:max-w-md">
        <Search className="absolute left-3 top-3 h-4 w-4 text-muted-foreground" />

        <Input
          className="pl-9"
          placeholder="Search monitor..."
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
        />
      </div>

      <Select
        value={status}
        onValueChange={(value) => onStatusChange(value)}
      >
        <SelectTrigger className="w-full md:w-44">
          <SelectValue />
        </SelectTrigger>

        <SelectContent>
          <SelectItem value="all">All</SelectItem>
          <SelectItem value="up">Up</SelectItem>
          <SelectItem value="down">Down</SelectItem>
        </SelectContent>
      </Select>
    </div>
  );
}

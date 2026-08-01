import type { ReactNode } from "react";
import { NavLink } from "react-router-dom";
import {
  BlocksIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  EyeIcon,
  EyeOffIcon,
} from "lucide-react";

import { cn } from "@/lib/utils";

export const SIDEBAR_SHOW_ALL_KEY = "skills-manage:show-all-platforms";

interface DashboardSidebarFrameProps {
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
  title: string;
  subtitle: string;
  navLabel: string;
  collapseLabel: string;
  expandLabel: string;
  children: ReactNode;
  footer?: ReactNode;
}

/** 데스크톱 앱과 웹 대시보드가 함께 사용하는 사이드바 외곽 틀이다. */
export function DashboardSidebarFrame({
  expanded,
  onExpandedChange,
  title,
  subtitle,
  navLabel,
  collapseLabel,
  expandLabel,
  children,
  footer,
}: DashboardSidebarFrameProps) {
  const toggleLabel = expanded ? collapseLabel : expandLabel;

  return (
    <nav
      className={cn(
        "flex h-full shrink-0 flex-col border-r border-border bg-sidebar text-sidebar-foreground transition-[width] duration-200",
        expanded ? "w-56" : "w-14",
      )}
      aria-label={navLabel}
    >
      <div
        className={cn(
          "flex items-center border-b border-border",
          expanded ? "justify-between px-3 py-2" : "justify-center py-2",
        )}
      >
        {expanded && (
          <div className="flex min-w-0 items-center gap-2">
            <BlocksIcon
              className="size-4 shrink-0 text-sidebar-primary"
              aria-hidden="true"
            />
            <div className="min-w-0">
              <p className="truncate text-sm font-bold tracking-tight text-sidebar-primary">
                {title}
              </p>
              <p className="truncate text-[10px] text-muted-foreground">{subtitle}</p>
            </div>
          </div>
        )}
        <button
          type="button"
          onClick={() => onExpandedChange(!expanded)}
          className="cursor-pointer rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground"
          aria-label={toggleLabel}
          title={toggleLabel}
        >
          {expanded ? (
            <ChevronLeftIcon className="size-4" />
          ) : (
            <ChevronRightIcon className="size-4" />
          )}
        </button>
      </div>

      <div className="flex-1 space-y-0.5 overflow-y-auto px-1.5 py-2">
        {children}
      </div>

      {footer && (
        <div className="space-y-0.5 border-t border-border px-1.5 py-2">
          {footer}
        </div>
      )}
    </nav>
  );
}

interface DashboardNavItemProps {
  label: string;
  icon: ReactNode;
  expanded: boolean;
  count?: number;
  title?: string;
  ariaLabel?: string;
  to?: string;
  end?: boolean;
  isActive?: boolean;
  onClick?: () => void;
}

/** 링크와 데스크톱 동작 버튼이 같은 모양을 사용하도록 맞춘다. */
export function DashboardNavItem({
  label,
  icon,
  expanded,
  count,
  title,
  ariaLabel,
  to,
  end,
  isActive = false,
  onClick,
}: DashboardNavItemProps) {
  const className = (active: boolean) =>
    cn(
      "relative flex w-full items-center rounded-md transition-colors",
      !active && "text-muted-foreground hover:bg-primary/10 hover:text-primary",
      active && "bg-hover-bg font-medium text-white",
      expanded ? "gap-2.5 px-2.5 py-1.5 text-sm" : "justify-center px-1.5 py-2",
    );

  const content = (active: boolean) => (
    <>
      <span className="shrink-0">{icon}</span>
      {expanded && (
        <>
          <span className="flex-1 truncate text-left">{label}</span>
          {count !== undefined && count > 0 && (
            <span
              className={cn(
                "shrink-0 rounded-full px-1.5 py-0.5 font-mono text-[10px] tabular-nums",
                active
                  ? "bg-white/20 text-white"
                  : "bg-muted/60 text-muted-foreground",
              )}
            >
              {count}
            </span>
          )}
        </>
      )}
      {active && (
        <span
          className="absolute top-1.5 bottom-1.5 left-0 w-0.5 rounded-r bg-white"
          aria-hidden="true"
        />
      )}
    </>
  );

  if (to) {
    return (
      <NavLink
        to={to}
        end={end}
        title={title ?? label}
        aria-label={ariaLabel ?? label}
        className={({ isActive: routeIsActive }) => className(routeIsActive)}
      >
        {({ isActive: routeIsActive }) => content(routeIsActive)}
      </NavLink>
    );
  }

  return (
    <button
      type="button"
      onClick={onClick}
      title={title ?? label}
      aria-label={ariaLabel ?? label}
      aria-current={isActive ? "page" : undefined}
      className={className(isActive)}
    >
      {content(isActive)}
    </button>
  );
}

export function DashboardSectionLabel({
  children,
  expanded,
  first = false,
}: {
  children: string;
  expanded: boolean;
  first?: boolean;
}) {
  if (!expanded) {
    return first ? null : <div className="my-1.5 border-t border-sidebar-border/40" />;
  }

  return (
    <div
      className={cn(
        "px-2.5 pb-1 text-[10px] font-medium tracking-wider text-muted-foreground/70 uppercase",
        !first && "pt-2",
      )}
    >
      {children}
    </div>
  );
}

export function DashboardPlatformToggle({
  expanded,
  showAll,
  onClick,
  showLabel,
  hideLabel,
}: {
  expanded: boolean;
  showAll: boolean;
  onClick: () => void;
  showLabel: string;
  hideLabel: string;
}) {
  const label = showAll ? hideLabel : showLabel;

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex w-full cursor-pointer items-center rounded-md text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground",
        expanded ? "gap-2.5 px-2.5 py-1.5 text-xs" : "justify-center px-1.5 py-2",
      )}
      aria-label={label}
      title={label}
    >
      {showAll ? (
        <EyeOffIcon className="size-4 shrink-0" />
      ) : (
        <EyeIcon className="size-4 shrink-0" />
      )}
      {expanded && <span>{label}</span>}
    </button>
  );
}

export function DashboardHeader({
  title,
  children,
}: {
  title: string;
  children?: ReactNode;
}) {
  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border bg-sidebar/60 px-4">
      <h1 className="min-w-0 flex-1 truncate text-sm font-semibold">{title}</h1>
      {children}
    </header>
  );
}

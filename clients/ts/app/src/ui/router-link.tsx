// Astryx's links through the router: an in-app href navigates without a reload, the rest stay links.

import type { Ref } from "react";
import { type ComponentProps, useMemo } from "react";
import { Link } from "@tanstack/react-router";

type Props = Omit<ComponentProps<"a">, "href"> & { href?: string; ref?: Ref<HTMLAnchorElement> };

function splitHref(href: string) {
  const url = new URL(href, "http://aurelius.local");
  return { to: url.pathname, search: Object.fromEntries(url.searchParams), hash: url.hash.slice(1) || undefined };
}

export function RouterLink({ href, children, ...rest }: Props) {
  const target = useMemo(() => (href?.startsWith("/") ? splitHref(href) : null), [href]);
  if (target === null) {
    return (
      <a href={href} {...rest}>
        {children}
      </a>
    );
  }
  return (
    <Link {...rest} to={target.to} search={target.search} hash={target.hash}>
      {children}
    </Link>
  );
}

// A loading state only shows once it has lasted a moment, so a fast answer never flashes one.

import { useEffect, useState } from "react";

export function useDelayed(active: boolean, ms = 150): boolean {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!active) {
      setShown(false);
      return;
    }
    const timer = setTimeout(() => setShown(true), ms);
    return () => clearTimeout(timer);
  }, [active, ms]);
  return shown;
}

import { createContext, useContext } from "react";

// The session gate picks the active garden while it renders and provides it here, so every garden
// screen below it has one from its first frame. The store only remembers the choice across launches.
export const ActiveGardenContext = createContext<string | null>(null);

/** The active garden's ID; only valid below the session gate. */
export function useActiveGardenId(): string {
  const id = useContext(ActiveGardenContext);
  if (!id) throw new Error("useActiveGardenId is used outside the session gate");
  return id;
}

import { createContext } from "react";

/** HUDScene's effective animation setting, including the host and system choice.
 * Standalone marks retain their own animation setting and system media query. */
export const HUDMotionContext = createContext(false);

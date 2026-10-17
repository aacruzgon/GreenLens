import { TZDate } from "@date-fns/tz";
import { format } from "date-fns";

/** Today as a LocalDate in the garden's time zone, never the phone's (docs/frontend 4, dates). */
export function localToday(timeZone: string, now: Date = new Date()): string {
  return format(new TZDate(now, timeZone), "yyyy-MM-dd");
}

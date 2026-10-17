import * as Location from "expo-location";

// The garden's location, with consent (architecture 8.5). Asked once, when creating a garden.
export type LocationResult = { kind: "ok"; lat: number; lon: number } | { kind: "denied" } | { kind: "unavailable" };

export async function currentLocation(): Promise<LocationResult> {
  try {
    const perm = await Location.requestForegroundPermissionsAsync();
    if (!perm.granted) return { kind: "denied" };
    const pos = await Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.Balanced });
    return { kind: "ok", lat: pos.coords.latitude, lon: pos.coords.longitude };
  } catch {
    return { kind: "unavailable" };
  }
}

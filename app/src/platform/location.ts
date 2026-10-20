import * as Location from "expo-location";

export type DeviceLocation = { lat: number; lon: number };

// Asks once, when the user taps "Use my location" (architecture 8.5). Null when refused or unavailable.
export async function requestGardenLocation(): Promise<DeviceLocation | null> {
  const { status } = await Location.requestForegroundPermissionsAsync();
  if (status !== "granted") return null;
  try {
    const position = await Location.getCurrentPositionAsync({ accuracy: Location.Accuracy.Balanced });
    return { lat: position.coords.latitude, lon: position.coords.longitude };
  } catch {
    return null;
  }
}

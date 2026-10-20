import AsyncStorage from "@react-native-async-storage/async-storage";

// Key-value storage for client state (docs/frontend/README.md §13). The only secret, Clerk's session, is in expo-secure-store.
export const storage = {
  get: (key: string) => AsyncStorage.getItem(key),
  set: (key: string, value: string) => AsyncStorage.setItem(key, value),
  remove: (key: string) => AsyncStorage.removeItem(key),
};

import AsyncStorage from "@react-native-async-storage/async-storage";
import { createJSONStorage } from "zustand/middleware";

// Device-local persistence for zustand stores (docs/frontend 12). Works in Expo Go and on the web.
export const persistStorage = createJSONStorage(() => AsyncStorage);

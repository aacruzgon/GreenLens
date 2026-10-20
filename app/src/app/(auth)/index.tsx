import { Redirect } from "expo-router";

// The signed-out entry point.
export default function AuthIndex() {
  return <Redirect href="/sign-in" />;
}

import { Redirect } from "expo-router";

// Where Google and Apple return (`pdm://sso-callback`, useSSO's default). The browser session takes the redirect,
// so this only answers a link that reaches the router; the auth guard then picks the screen.
export default function SsoCallback() {
  return <Redirect href="/" />;
}

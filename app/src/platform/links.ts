// Opening what a result points at: a source's page in the in-app browser (Safari View Controller on iOS), and a
// farm record in the app that holds it (Slack or Google Drive), which the system picks from the link.
import { Linking } from "react-native";
import * as WebBrowser from "expo-web-browser";

export const links = {
  async openInBrowser(url: string): Promise<void> {
    await WebBrowser.openBrowserAsync(url);
  },
  async openExternal(url: string): Promise<void> {
    await Linking.openURL(url);
  },
};

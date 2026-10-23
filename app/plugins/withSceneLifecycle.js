// iOS 27 stops an app at launch unless it adopts the scene life cycle
// (UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption). Expo SDK 57 ships the scene delegate,
// `ExpoAppSceneDelegate`, but its prebuild template doesn't use it yet (57.0.33), so this plugin does:
// - Info.plist names the scene delegate in a single-scene manifest;
// - AppDelegate conforms to `ExpoReactNativeFactoryProvider`, so the scene delegate finds the factory, and no
//   longer makes the window itself: the scene delegate creates it from the connecting scene and starts React Native.
// Remove the plugin once Expo's template does this.
const { withAppDelegate, withInfoPlist } = require("expo/config-plugins");

const SCENE_DELEGATE = "EXExpoAppSceneDelegate";

const WINDOW_BLOCK = /#if os\(iOS\) \|\| os\(tvOS\)\n\s*window = UIWindow\(frame: UIScreen\.main\.bounds\)\n\s*factory\.startReactNative\([\s\S]*?\)\n#endif\n/;

function withSceneManifest(config) {
  return withInfoPlist(config, (mod) => {
    mod.modResults.UIApplicationSceneManifest = {
      UIApplicationSupportsMultipleScenes: false,
      UISceneConfigurations: {
        UIWindowSceneSessionRoleApplication: [
          { UISceneConfigurationName: "Default", UISceneDelegateClassName: SCENE_DELEGATE },
        ],
      },
    };
    return mod;
  });
}

function withSceneAppDelegate(config) {
  return withAppDelegate(config, (mod) => {
    if (mod.modResults.language !== "swift") throw new Error("withSceneLifecycle expects a Swift AppDelegate.");
    let src = mod.modResults.contents;
    if (!src.includes("ExpoReactNativeFactoryProvider")) {
      const declaration = "class AppDelegate: ExpoAppDelegate {";
      if (!src.includes(declaration)) throw new Error("withSceneLifecycle: AppDelegate's declaration has changed.");
      src = src.replace(declaration, "class AppDelegate: ExpoAppDelegate, ExpoReactNativeFactoryProvider {");
    }
    if (WINDOW_BLOCK.test(src)) {
      src = src.replace(WINDOW_BLOCK, "    // ExpoAppSceneDelegate creates the window and starts React Native in it.\n");
    } else if (src.includes("window = UIWindow(frame:")) {
      throw new Error("withSceneLifecycle: AppDelegate's window set-up has changed.");
    }
    mod.modResults.contents = src;
    return mod;
  });
}

module.exports = function withSceneLifecycle(config) {
  return withSceneAppDelegate(withSceneManifest(config));
};

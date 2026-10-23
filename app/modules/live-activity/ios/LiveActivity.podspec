Pod::Spec.new do |s|
  s.name           = 'LiveActivity'
  s.version        = '1.0.0'
  s.summary        = "Starts, updates and ends the scan's Live Activity (roadmap F5 item 6)."
  s.description    = "The app side of the scan's Live Activity; the widget extension is targets/scan-activity."
  s.license        = 'UNLICENSED'
  s.author         = 'PDM'
  s.homepage       = 'https://github.com/aacruzgon/GreenLens'
  s.platforms      = {
    :ios => '16.4'
  }
  s.swift_version  = '5.9'
  s.source         = { git: '' }
  s.static_framework = true

  s.dependency 'ExpoModulesCore'
  s.frameworks = 'ActivityKit'

  s.source_files = "**/*.{h,m,swift}"
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'SWIFT_COMPILATION_MODE' => 'wholemodule'
  }
end

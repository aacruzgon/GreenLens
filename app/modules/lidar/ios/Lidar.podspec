# The LiDAR module (docs/frontend/README.md 13; architecture 6.7, 6.8): depth capture, the garden mapping session
# and the 3D garden, on the iPhone only.
Pod::Spec.new do |s|
  s.name           = 'Lidar'
  s.version        = '1.0.0'
  s.summary        = 'PDM LiDAR capture, garden mapping and the 3D garden'
  s.license        = 'UNLICENSED'
  s.author         = 'PDM'
  s.homepage       = 'https://github.com/aacruzgon/GreenLens'
  s.platforms      = { :ios => '16.4' }
  s.source         = { git: '' }
  s.static_framework = true
  s.dependency 'ExpoModulesCore'
  s.frameworks     = 'AVFoundation', 'CoreMotion', 'ARKit', 'RealityKit', 'ImageIO'
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES', 'SWIFT_COMPILATION_MODE' => 'wholemodule' }
  s.source_files   = '**/*.{h,m,swift}'
end

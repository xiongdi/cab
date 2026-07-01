cask "cab-desktop" do
  version "0.5.1"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with actual DMG checksum on release

  url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-desktop_#{version}_universal.dmg"
  name "CAB"
  desc "Coding Agents Bridge - Local LLM Gateway Router for Coding Agent CLIs"
  homepage "https://github.com/xiongdi/cab"

  app "cab-desktop.app"

  # Automatically symlink CLI tools from the desktop package into Homebrew's binary path (/usr/local/bin)
  binary "#{appdir}/cab-desktop.app/Contents/Resources/cab"
  binary "#{appdir}/cab-desktop.app/Contents/Resources/cabd"

  zap trash: [
    "~/.cab",
    "~/Library/Application Support/com.cab.gateway",
    "~/Library/Preferences/com.cab.gateway.plist",
  ]
end

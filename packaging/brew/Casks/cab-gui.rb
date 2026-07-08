cask "cab-gui" do
  version "0.6.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with actual DMG checksum on release

  url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-gui_#{version}_universal.dmg"
  name "CAB"
  desc "Coding Agents Bridge - Local LLM Gateway Router for Coding Agent CLIs"
  homepage "https://github.com/xiongdi/cab"

  app "cab-gui.app"

  # Automatically symlink CLI tools from the desktop package into Homebrew's binary path (/usr/local/bin)
  binary "#{appdir}/cab-gui.app/Contents/Resources/cab-cli"
  binary "#{appdir}/cab-gui.app/Contents/Resources/cab-srv"

  zap trash: [
    "~/.cab",
    "~/Library/Application Support/com.cab.gateway",
    "~/Library/Preferences/com.cab.gateway.plist",
  ]
end

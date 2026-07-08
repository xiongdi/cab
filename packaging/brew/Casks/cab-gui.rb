cask "cab-gui" do
  version "0.6.0"

  on_intel do
    sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with Intel DMG checksum
    url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-gui_#{version}_x64.dmg"
  end
  on_arm do
    sha256 "1111111111111111111111111111111111111111111111111111111111111111" # Replace with Apple Silicon DMG checksum
    url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-gui_#{version}_arm64.dmg"
  end

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

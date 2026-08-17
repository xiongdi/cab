class Cab < Formula
  desc "Coding Agents Bridge - Local LLM Gateway Router for Coding Agent CLIs"
  homepage "https://github.com/xiongdi/cab"
  version "0.10.10"
  license "ACL-1.0"

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-linux-x64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace on release
    elsif Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-linux-arm64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace on release
    end
  end

  def install
    bin.install "cab"
    (share/"cab").install "ui" if Dir.exist?("ui")
  end

  def post_install
    ohai "To install and start the cab daemon service, run:"
    ohai "  cab service install"
    ohai "  cab start"
    ohai "  cab gui"
  end

  test do
    assert_match "Coding Agents Bridge", shell_output("#{bin}/cab --help")
  end
end

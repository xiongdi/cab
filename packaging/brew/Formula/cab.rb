class Cab < Formula
  desc "Coding Agents Bridge - Local LLM Gateway Router for Coding Agent CLIs"
  homepage "https://github.com/xiongdi/cab"
  version "0.6.0"
  license "ACL-1.0"

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-v#{version}-linux-x64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with Linux x64 binary checksum on release
    elsif Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-v#{version}-linux-arm64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with Linux arm64 binary checksum on release
    end
  end

  def install
    # Install the precompiled cab-cli and cab-srv binaries directly
    bin.install "cab-cli"
    bin.install "cab-srv"
  end

  def post_install
    ohai "To install and start the cab-srv daemon service, run:"
    ohai "  cab-cli service install"
    ohai "  cab-cli start"
  end

  test do
    assert_match "cab-cli version", shell_output("#{bin}/cab-cli --version")
  end
end

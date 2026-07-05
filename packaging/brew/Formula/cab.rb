class Cab < Formula
  desc "Coding Agents Bridge - Local LLM Gateway Router for Coding Agent CLIs"
  homepage "https://github.com/xiongdi/cab"
  version "0.6.0"
  license "ACL-1.0"

  if OS.mac?
    url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-v#{version}-macos-universal.tar.gz"
    sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with macOS universal binary checksum on release
  elsif OS.linux?
    if Hardware::CPU.intel?
      url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-v#{version}-linux-x64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with Linux x64 binary checksum on release
    elsif Hardware::CPU.arm? && Hardware::CPU.is_64_bit?
      url "https://github.com/xiongdi/cab/releases/download/v#{version}/cab-v#{version}-linux-arm64.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with Linux arm64 binary checksum on release
    end
  end

  def install
    # Install the precompiled cab and cabd binaries directly
    bin.install "cab"
    bin.install "cabd"
  end

  def post_install
    ohai "To install and start the cabd daemon service, run:"
    ohai "  cab service install"
    ohai "  cab start"
  end

  test do
    assert_match "cab version", shell_output("#{bin}/cab --version")
  end
end

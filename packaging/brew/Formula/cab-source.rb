class Cab < Formula
  desc "Coding Agents Bridge - Local LLM Gateway Router for Coding Agent CLIs"
  homepage "https://github.com/xiongdi/cab"
  url "https://github.com/xiongdi/cab/archive/refs/tags/v0.6.0.tar.gz"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000" # Replace with source tarball checksum on release
  license "ACL-1.0"

  depends_on "node@24" => :build
  depends_on "rust" => :build

  def install
    # Install Svelte UI assets and build
    system "npm", "install"
    system "npm", "run", "build"

    # Build release binaries
    system "cargo", "build", "--release", "-p", "cab", "-p", "cab-srv"

    bin.install "target/release/cab-cli"
    bin.install "target/release/cab-srv"
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

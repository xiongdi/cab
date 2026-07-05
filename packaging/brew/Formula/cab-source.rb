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
    system "cargo", "build", "--release", "-p", "cab", "-p", "cab-server"

    bin.install "target/release/cab"
    bin.install "target/release/cabd"
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

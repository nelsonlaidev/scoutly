const { createHash } = require("node:crypto");
const { createReadStream, createWriteStream } = require("node:fs");
const { chmod, copyFile, mkdir, mkdtemp, readFile, rm } = require("node:fs/promises");
const { tmpdir } = require("node:os");
const path = require("node:path");
const { Readable } = require("node:stream");
const { pipeline } = require("node:stream/promises");

const AdmZip = require("adm-zip");
const tar = require("tar");
const { EnvHttpProxyAgent, fetch } = require("undici");

const packageJson = require("./package.json");

const releaseRepository = "https://github.com/nelsonlaidev/scoutly";
const artifacts = {
  "darwin-arm64": "scoutly_Darwin_arm64.tar.gz",
  "darwin-x64": "scoutly_Darwin_x86_64.tar.gz",
  "linux-arm64": "scoutly_Linux_arm64.tar.gz",
  "linux-ia32": "scoutly_Linux_i386.tar.gz",
  "linux-x64": "scoutly_Linux_x86_64.tar.gz",
  "win32-arm64": "scoutly_Windows_arm64.zip",
  "win32-ia32": "scoutly_Windows_i386.zip",
  "win32-x64": "scoutly_Windows_x86_64.zip",
};

function artifactFor(platform, arch) {
  const artifact = artifacts[`${platform}-${arch}`];
  if (!artifact) {
    throw new Error(`Scoutly does not support ${platform}-${arch}`);
  }
  return artifact;
}

function checksumFor(contents, artifact) {
  for (const line of contents.split(/\r?\n/)) {
    const [checksum, filename] = line.trim().split(/\s+/, 2);
    if (filename === artifact && /^[a-f0-9]{64}$/i.test(checksum)) {
      return checksum.toLowerCase();
    }
  }
  throw new Error(`No SHA-256 checksum found for ${artifact}`);
}

async function download(url, destination, dispatcher) {
  const response = await fetch(url, { dispatcher, redirect: "follow" });
  if (!response.ok || !response.body) {
    throw new Error(`Download failed with HTTP ${response.status}: ${url}`);
  }
  await pipeline(Readable.fromWeb(response.body), createWriteStream(destination));
}

async function sha256(filename) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filename)) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

async function extract(archive, destination) {
  if (archive.endsWith(".zip")) {
    new AdmZip(archive).extractAllTo(destination, true);
    return;
  }
  await tar.x({ file: archive, cwd: destination, strict: true });
}

async function install() {
  if (packageJson.version === "0.0.0") {
    console.warn("Skipping the Scoutly binary download for the development package.");
    return;
  }

  const artifact = artifactFor(process.platform, process.arch);
  const binaryName = process.platform === "win32" ? "scoutly.exe" : "scoutly";
  const releaseBase = `${releaseRepository}/releases/download/v${packageJson.version}`;
  const checksumAsset = `scoutly_${packageJson.version}_checksums.txt`;
  const temporaryDirectory = await mkdtemp(path.join(tmpdir(), "scoutly-"));
  const dispatcher = new EnvHttpProxyAgent();

  try {
    const archivePath = path.join(temporaryDirectory, artifact);
    const checksumPath = path.join(temporaryDirectory, checksumAsset);
    await download(`${releaseBase}/${checksumAsset}`, checksumPath, dispatcher);
    await download(`${releaseBase}/${artifact}`, archivePath, dispatcher);

    const expectedChecksum = checksumFor(await readFile(checksumPath, "utf8"), artifact);
    const actualChecksum = await sha256(archivePath);
    if (actualChecksum !== expectedChecksum) {
      throw new Error(`SHA-256 checksum mismatch for ${artifact}`);
    }

    await extract(archivePath, temporaryDirectory);
    const binDirectory = path.join(__dirname, "bin");
    const installedBinary = path.join(binDirectory, binaryName);
    await mkdir(binDirectory, { recursive: true });
    await copyFile(path.join(temporaryDirectory, binaryName), installedBinary);
    if (process.platform !== "win32") {
      await chmod(installedBinary, 0o755);
    }
  } finally {
    try {
      await dispatcher.close();
    } finally {
      await rm(temporaryDirectory, { recursive: true, force: true });
    }
  }
}

if (require.main === module) {
  install().catch((error) => {
    console.error(`Failed to install Scoutly: ${error.message}`);
    process.exitCode = 1;
  });
}

module.exports = { artifactFor, checksumFor };

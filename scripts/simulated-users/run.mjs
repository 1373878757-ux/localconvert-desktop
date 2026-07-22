import { spawn, spawnSync } from "node:child_process";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "../..");
const reportsDirectory = join(repositoryRoot, "reports/simulated-users");
const workDirectory = join(reportsDirectory, ".work");
const resultsPath = join(reportsDirectory, "latest-results.json");
const summaryPath = join(reportsDirectory, "latest-summary.md");
const passthroughArguments = process.argv.slice(2).filter((value) => value !== "--");
const observationsPath = argumentValue(passthroughArguments, "--record-observations");
const profile = argumentValue(passthroughArguments, "--profile");

await mkdir(workDirectory, { recursive: true });
if (observationsPath) {
  const aggregate = await readAggregate();
  const observations = JSON.parse(await readFile(resolve(observationsPath), "utf8"));
  aggregate.generatedAt = new Date().toISOString();
  aggregate.coldLaunches = observations.coldLaunches ?? aggregate.coldLaunches;
  aggregate.guiSmoke = observations.guiSmoke ?? aggregate.guiSmoke;
  aggregate.deferredProfiles = deferredProfiles();
  await writeJsonAtomically(resultsPath, aggregate);
  await writeFile(summaryPath, renderMarkdown(aggregate), "utf8");
  console.log(`recorded observations in ${relative(resultsPath)}`);
  process.exit(0);
}

if (!profile || !["quick", "standard", "stress", "soak"].includes(profile)) {
  fail("--profile must be one of quick, standard, stress, or soak.");
}

const timestamp = new Date().toISOString().replaceAll(":", "-");
const campaignPath = join(workDirectory, `${profile}-${timestamp}.json`);
const cargoArguments = [
  "run",
  "--quiet",
  "--manifest-path",
  join(repositoryRoot, "src-tauri/Cargo.toml"),
  "--features",
  "simulated-users",
  "--bin",
  "simulated-users",
  "--",
  ...passthroughArguments,
  "--result-path",
  campaignPath
];

const child = spawn("cargo", cargoArguments, {
  cwd: repositoryRoot,
  detached: process.platform !== "win32",
  env: { ...process.env, NO_COLOR: process.env.NO_COLOR ?? "1" },
  shell: false,
  stdio: "inherit"
});

let stopping = false;
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    if (stopping) return;
    stopping = true;
    stopChild(child);
  });
}

const exitCode = await new Promise((accept) => {
  child.once("error", (error) => fail(`Unable to start Rust runner: ${error.message}`));
  child.once("exit", (code, signal) => accept(code ?? (signal ? 128 : 1)));
});

if (existsSync(campaignPath)) {
  const campaign = JSON.parse(await readFile(campaignPath, "utf8"));
  const aggregate = await readAggregate();
  aggregate.generatedAt = new Date().toISOString();
  aggregate.campaigns[profile] = campaign;
  aggregate.deferredProfiles = deferredProfiles();
  await writeJsonAtomically(resultsPath, aggregate);
  await writeFile(summaryPath, renderMarkdown(aggregate), "utf8");
  console.log(`aggregate JSON: ${relative(resultsPath)}`);
  console.log(`summary: ${relative(summaryPath)}`);
}

if (exitCode !== 0) {
  process.exitCode = exitCode;
}

function argumentValue(argumentsList, name) {
  const index = argumentsList.indexOf(name);
  return index >= 0 ? argumentsList[index + 1] : undefined;
}

function stopChild(runningChild) {
  if (!runningChild.pid) return;
  try {
    if (process.platform === "win32") runningChild.kill("SIGTERM");
    else process.kill(-runningChild.pid, "SIGTERM");
  } catch {
    // The process may already have completed.
  }
}

async function readAggregate() {
  if (!existsSync(resultsPath)) {
    return {
      schemaVersion: 1,
      evidenceType: "deterministic simulated-user workload; not real-user evidence",
      generatedAt: new Date().toISOString(),
      campaigns: {},
      coldLaunches: [],
      guiSmoke: [],
      deferredProfiles: deferredProfiles()
    };
  }
  const current = JSON.parse(await readFile(resultsPath, "utf8"));
  return {
    schemaVersion: 1,
    evidenceType: "deterministic simulated-user workload; not real-user evidence",
    generatedAt: current.generatedAt ?? new Date().toISOString(),
    campaigns: current.campaigns ?? {},
    coldLaunches: current.coldLaunches ?? [],
    guiSmoke: current.guiSmoke ?? [],
    deferredProfiles: current.deferredProfiles ?? deferredProfiles()
  };
}

function deferredProfiles() {
  return [
    {
      profile: "stress",
      reason: "Deferred until at least 20 GiB free disk space is available and --allow-stress is explicit.",
      command: "npm run test:simulated-users:stress -- --allow-stress --seed 0x4c43443039300003"
    },
    {
      profile: "soak",
      reason: "Deferred until at least 20 GiB free disk space is available and --allow-soak is explicit.",
      command: "npm run test:simulated-users:soak -- --allow-soak --duration-minutes 120 --seed 0x4c43443039300004"
    }
  ];
}

async function writeJsonAtomically(path, value) {
  const temporaryPath = `${path}.tmp`;
  await writeFile(temporaryPath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  await rename(temporaryPath, path);
}

function renderMarkdown(result) {
  const campaigns = Object.values(result.campaigns);
  const totals = campaigns.reduce(
    (sum, item) => ({
      sessions: sum.sessions + (item.virtualSessions ?? 0),
      attempts: sum.attempts + (item.taskAttempts ?? 0),
      violations: sum.violations + (item.invariantViolations?.length ?? 0),
      crashes: sum.crashes + (item.crashes ?? 0),
      timeouts: sum.timeouts + (item.timeouts ?? 0)
    }),
    { sessions: 0, attempts: 0, violations: 0, crashes: 0, timeouts: 0 }
  );
  const verdict = campaigns.length > 0 && campaigns.every((item) => item.verdict === "pass")
    ? "PASS"
    : "INCOMPLETE OR FAILED";
  const profileRows = campaigns
    .map((item) => `| ${item.profile} | ${item.seed} | ${item.virtualSessions} | ${item.taskAttempts} | ${item.concurrency.join(", ")} | ${item.durationMs} ms | ${item.verdict} |`)
    .join("\n") || "| Not run | - | - | - | - | - | - |";
  const operationRows = distributionRows(campaigns, "operationDistribution");
  const statusRows = distributionRows(campaigns, "statusDistribution");
  const resources = campaigns.map((item) => {
    const observation = item.resourceObservations ?? {};
    return `- **${item.profile}:** peak RSS ${formatKib(observation.peakRssKib)}, peak CPU ${formatOptional(observation.peakCpuPercent, "%")}, peak FDs ${formatOptional(observation.peakOpenFileDescriptors)}, peak child processes ${formatOptional(observation.peakChildProcesses)}, peak temporary data ${formatBytes(observation.peakTemporaryBytes)}.`;
  }).join("\n") || "- Not measured yet.";
  const coldLaunchRows = result.coldLaunches
    .map((item) => `| ${item.launch} | ${item.durationMs} ms | ${yesNo(item.splashSeen)} | ${yesNo(item.mainWindow)} | ${yesNo(item.qpdfAvailable)} | ${yesNo(item.imageEngineAvailable)} | ${yesNo(!item.unexpectedStartupError)} |`)
    .join("\n") || "| Not run | - | - | - | - | - | - |";
  const guiRows = result.guiSmoke
    .map((item) => `| ${item.check} | ${item.result} | ${item.note} |`)
    .join("\n") || "| Not run | unavailable | No GUI observation recorded. |";
  const deferred = result.deferredProfiles.map((item) => `- **${item.profile}:** ${item.reason}\n  \`${item.command}\``).join("\n");
  return `# Simulated User Stability Summary\n\n> This is deterministic simulated workload evidence, not evidence from real users. Absolute paths are omitted or replaced with \`<repo-root>\` and \`<simulation-root>\`.\n\n- **Generated:** ${result.generatedAt}\n- **Verdict:** ${verdict}\n- **Covered:** ${totals.sessions} virtual sessions and ${totals.attempts} real backend task attempts\n- **Invariant violations:** ${totals.violations}\n- **Crashes / timeouts:** ${totals.crashes} / ${totals.timeouts}\n\n## Profiles\n\n| Profile | Seed | Sessions | Attempts | Concurrency | Duration | Verdict |\n| --- | --- | ---: | ---: | --- | ---: | --- |\n${profileRows}\n\n## Operation Distribution\n\n| Operation | Attempts |\n| --- | ---: |\n${operationRows}\n\n## Status Distribution\n\n| Status | Attempts |\n| --- | ---: |\n${statusRows}\n\n## Resource Observations\n\n${resources}\n\nUnavailable measurements remain explicit in the JSON report. Resource data is sampled from local \`ps\`, \`lsof\`, \`df\`, and macOS memory tools without shell command strings.\n\n## Cold Launch Results\n\n| Launch | Main-window time | Splash | Main window | qpdf | image-engine | No startup error |\n| ---: | ---: | --- | --- | --- | --- | --- |\n${coldLaunchRows}\n\n## GUI Smoke Results\n\n| Check | Result | Note |\n| --- | --- | --- |\n${guiRows}\n\nUI automation restrictions are reported separately and are not treated as product failures.\n\n## Safety Invariants\n\n- Source SHA-256 changes: ${campaigns.reduce((sum, item) => sum + (item.sourceHashChanges ?? 0), 0)}.\n- Output overwrite violations: ${campaigns.reduce((sum, item) => sum + (item.outputOverwriteViolations ?? 0), 0)}.\n- Temporary directory leaks: ${campaigns.reduce((sum, item) => sum + (item.temporaryDirectoryLeaks ?? 0), 0)}.\n- Network observation: ${result.guiSmoke.find((item) => item.check === "packaged-app network")?.note ?? "not reliably measured"}.\n\n## Deferred Heavy Profiles\n\n${deferred}\n`;
}

function distributionRows(campaigns, property) {
  const totals = new Map();
  for (const campaign of campaigns) {
    for (const [name, count] of Object.entries(campaign[property] ?? {})) {
      totals.set(name, (totals.get(name) ?? 0) + count);
    }
  }
  return [...totals.entries()].sort(([left], [right]) => left.localeCompare(right)).map(([name, count]) => `| ${name} | ${count} |`).join("\n") || "| Not run | 0 |";
}

function formatOptional(value, suffix = "") {
  return value === null || value === undefined ? "unavailable" : `${value}${suffix}`;
}

function formatKib(value) {
  return value === null || value === undefined ? "unavailable" : `${(value / 1024).toFixed(1)} MiB`;
}

function formatBytes(value) {
  return value === null || value === undefined ? "unavailable" : `${(value / 1024 / 1024).toFixed(1)} MiB`;
}

function yesNo(value) {
  return value ? "yes" : "no";
}

function relative(path) {
  return path.replace(`${repositoryRoot}/`, "<repo-root>/");
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

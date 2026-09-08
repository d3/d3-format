#Requires -Version 5.1

<#
.SYNOPSIS
    Builds and runs the downstream canaries against the packaged crate.

.DESCRIPTION
    Section 7's Phase 6 canary list, points 1 to 3: two downstream sample
    applications -- one default-feature, one all-feature -- built from the unpacked
    .crate rather than from the workspace source, plus a multithreaded formatter
    workload and a locale-heavy workload.

    "Built from the packaged crate" is the part that is easy to claim and easy to get
    wrong, so it is asserted three ways rather than assumed:

    - The dependency is a path to .packaged/d3-format, which this script creates by
      extracting the tarball. It is deleted and recreated on every run, so a stale
      copy cannot survive.
    - The extraction is checked against the exclude list. If rust-toolchain.toml,
      fixtures/, tools/, ci/ or src/bin/ appeared, the tarball is not the one section
      7 describes and the canary would be measuring something else.
    - `cargo metadata` is read back and the manifest path cargo actually resolved for
      d3-format must be the extracted one. Nothing else proves that a stray
      [patch] or a path typo did not send the build at rust/ after all.

    Each consumer is then built and run twice: once on the pinned toolchain and once
    on the declared MSRV. The MSRV pass is the reason section 7 excludes
    rust-toolchain.toml from the package, and it is only meaningful from here --
    inside rust/ the pin applies and 1.80.0 can never be what runs.

    Authored for Windows PowerShell 5.1 as well as PowerShell 7, like ci/check.ps1,
    and it contains no Bash syntax.

.PARAMETER SkipPackage
    Reuse the .crate already in rust/target/package/ instead of building a new one.

.PARAMETER AllowDirty
    Passes --allow-dirty to `cargo package`, as ci/check.ps1 does: a local run always
    has uncommitted work in progress.

.PARAMETER Release
    Build the consumers with --release. The default is the dev profile, which keeps
    Rust's overflow checks and debug assertions live -- the same reasoning that makes
    the per-PR differential a debug run.

.PARAMETER MsrvToolchain
    The exact toolchain the MSRV pass runs. It has to be spelled in full: rustup
    reads `1.80` as "the newest 1.80.x" and installs 1.80.1, which is not the
    compiler .github/workflows/test.yml gates on. The script refuses a value that
    does not begin with the rust-version in rust/Cargo.toml, so raising the declared
    MSRV cannot leave this behind.

.EXAMPLE
    ./run-canaries.ps1 -AllowDirty
#>

[CmdletBinding()]
param(
    [switch] $SkipPackage,

    [switch] $AllowDirty,

    [switch] $Release,

    [string] $MsrvToolchain = '1.80.0'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

$Consumers = @('default-consumer', 'all-features-consumer')

# Paths that must not be in the extraction. Same list as the packaging collector
# asserts, checked again here because the two run independently and a canary that
# silently built a tarball carrying rust-toolchain.toml would be testing the pinned
# toolchain twice while reporting an MSRV result.
$MustNotExist = @('rust-toolchain.toml', 'fixtures', 'tools', 'ci', 'src/bin')

# ...and paths that must be, because their absence would make the canary pass for the
# wrong reason: a crate missing its generated locale table still builds without the
# `locales` feature.
$MustExist = @('Cargo.toml', 'src/lib.rs', 'src/locales/generated.rs', 'README.md')

function Invoke-Step {
    <#
        As ci/check.ps1: a native command's exit code does not stop a PowerShell
        script on its own, and $ErrorActionPreference only covers native exit codes
        on PowerShell 7.4 and later.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [string] $Command,
        [Parameter(Mandatory = $true)] [AllowEmptyCollection()] [string[]] $Arguments
    )

    Write-Host ''
    Write-Host "==> $Command $($Arguments -join ' ')"
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

function Get-CommandOutput {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [string] $Command,
        [Parameter(Mandatory = $true)] [AllowEmptyCollection()] [string[]] $Arguments
    )

    $captured = & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
    , @($captured)
}

$canaryRoot = $PSScriptRoot
$repositoryRoot = Split-Path -Parent $canaryRoot
$crateRoot = Join-Path $repositoryRoot 'rust'
$packagedRoot = Join-Path $canaryRoot '.packaged'
$unpacked = Join-Path $packagedRoot 'd3-format'

$manifest = Get-Content -LiteralPath (Join-Path $crateRoot 'Cargo.toml') -Raw
if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
    throw 'rust/Cargo.toml has no package version this script can read'
}
$version = $Matches[1]

$toolchainFile = Get-Content -LiteralPath (Join-Path $crateRoot 'rust-toolchain.toml') -Raw
if ($toolchainFile -notmatch '(?m)^channel\s*=\s*"([^"]+)"') {
    throw 'rust/rust-toolchain.toml has no channel this script can read'
}
# Read rather than hardcoded, so bumping the pin in one place cannot leave the
# canary silently testing the old compiler.
$pinnedToolchain = $Matches[1]

if ($manifest -notmatch '(?m)^rust-version\s*=\s*"([^"]+)"') {
    throw 'rust/Cargo.toml has no rust-version this script can read'
}
$declaredMsrv = $Matches[1]
if (-not $MsrvToolchain.StartsWith($declaredMsrv)) {
    throw "the MSRV toolchain $MsrvToolchain does not match rust-version = `"$declaredMsrv`" in rust/Cargo.toml"
}

$cratePath = Join-Path $crateRoot "target/package/d3-format-$version.crate"

Write-Host 'd3-format downstream canaries'
Write-Host "  version:  $version"
Write-Host "  crate:    $cratePath"
Write-Host "  unpacked: $unpacked"
Write-Host "  profile:  $(if ($Release) { 'release' } else { 'dev' })"

try {
    if (-not $SkipPackage) {
        $packageArguments = @('package', '--locked', '--manifest-path', (Join-Path $crateRoot 'Cargo.toml'))
        if ($AllowDirty) {
            $packageArguments += '--allow-dirty'
        }
        # From the crate directory, so rust/.cargo/config.toml is the one in effect,
        # exactly as ci/check.ps1 packages.
        Push-Location -LiteralPath $crateRoot
        try {
            Invoke-Step -Command 'cargo' -Arguments $packageArguments
        }
        finally {
            Pop-Location
        }
    }

    if (-not (Test-Path -LiteralPath $cratePath)) {
        throw "no packaged crate at $cratePath; run without -SkipPackage"
    }
    $crateSha = (Get-FileHash -LiteralPath $cratePath -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Host "  sha256:   $crateSha"

    # Deleted and recreated every run. A canary that had been quietly building last
    # week's tarball would pass for the wrong reason.
    if (Test-Path -LiteralPath $packagedRoot) {
        Remove-Item -LiteralPath $packagedRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Path $unpacked -Force | Out-Null

    # --strip-components=1 drops the d3-format-<version>/ prefix, which is what keeps
    # the path in each consumer's Cargo.toml free of the version number.
    Invoke-Step -Command 'tar' -Arguments @('-xzf', $cratePath, '-C', $unpacked, '--strip-components=1')

    foreach ($relative in $MustNotExist) {
        $path = Join-Path $unpacked $relative
        if (Test-Path -LiteralPath $path) {
            throw "the packaged crate contains $relative, which Cargo.toml excludes; the canary would not be testing what a consumer downloads"
        }
    }
    foreach ($relative in $MustExist) {
        $path = Join-Path $unpacked $relative
        if (-not (Test-Path -LiteralPath $path)) {
            throw "the packaged crate is missing $relative"
        }
    }

    $unpackedManifest = Get-Content -LiteralPath (Join-Path $unpacked 'Cargo.toml') -Raw
    if ($unpackedManifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') {
        throw 'the packaged Cargo.toml has no version'
    }
    if ($Matches[1] -ne $version) {
        throw "the packaged crate is version $($Matches[1]) but rust/Cargo.toml says $version"
    }
    Write-Host ''
    Write-Host "    extraction verified: version $version, $($MustNotExist.Count) exclusion(s) held"

    foreach ($consumer in $Consumers) {
        $consumerRoot = Join-Path $canaryRoot $consumer
        Push-Location -LiteralPath $consumerRoot
        try {
            Write-Host ''
            Write-Host "--- $consumer ---"

            # Section 1: the lockfile is regenerated only under the pinned toolchain,
            # because cargo 1.80 ignores resolver.incompatible-rust-versions and would
            # reselect a dependency it cannot parse. The MSRV pass below consumes this
            # lock with --locked and never writes one.
            $env:RUSTUP_TOOLCHAIN = $pinnedToolchain
            Invoke-Step -Command 'cargo' -Arguments @('generate-lockfile')

            # The assertion that this is the packaged crate. cargo reports the manifest
            # it actually resolved, so a path typo, a leftover [patch] or a directory
            # that failed to extract all show up here rather than as a silent pass
            # against rust/src.
            $metadata = (Get-CommandOutput -Command 'cargo' -Arguments @(
                    'metadata', '--locked', '--format-version', '1')) -join "`n" | ConvertFrom-Json
            $dependency = @($metadata.packages | Where-Object { $_.name -eq 'd3-format' })
            if ($dependency.Count -ne 1) {
                throw "expected exactly one d3-format package in the graph, found $($dependency.Count)"
            }
            $resolved = (Resolve-Path -LiteralPath $dependency[0].manifest_path).Path
            $expected = (Resolve-Path -LiteralPath (Join-Path $unpacked 'Cargo.toml')).Path
            if ($resolved -ne $expected) {
                throw (@(
                    'the consumer did not resolve d3-format to the packaged crate.',
                    "  resolved : $resolved",
                    "  expected : $expected"
                ) -join [Environment]::NewLine)
            }
            if ($dependency[0].version -ne $version) {
                throw "the resolved d3-format is $($dependency[0].version), not $version"
            }
            Write-Host "    resolved d3-format $version from $resolved"

            # The default consumer's whole graph must be itself and the crate: section
            # 5.1 says the default build has no dependencies, and README.md says so to
            # every crates.io reader.
            if ($consumer -eq 'default-consumer') {
                $names = @($metadata.packages | ForEach-Object { $_.name } | Sort-Object)
                $expectedNames = @('d3-format', 'default-consumer')
                if (($names -join ', ') -ne ($expectedNames -join ', ')) {
                    throw "the default-feature build is supposed to have no dependencies, but the graph is: $($names -join ', ')"
                }
                Write-Host '    default-feature graph is d3-format and nothing else'
            }

            foreach ($toolchain in @($pinnedToolchain, $MsrvToolchain)) {
                $env:RUSTUP_TOOLCHAIN = $toolchain
                Write-Host ''
                Write-Host "  RUSTUP_TOOLCHAIN=$toolchain"
                Invoke-Step -Command 'rustc' -Arguments @('--version')

                $runArguments = @('run', '--locked', '--quiet')
                if ($Release) {
                    $runArguments += '--release'
                }
                Invoke-Step -Command 'cargo' -Arguments $runArguments
            }
        }
        finally {
            Remove-Item Env:\RUSTUP_TOOLCHAIN -ErrorAction SilentlyContinue
            Pop-Location
        }
    }

    Write-Host ''
    Write-Host "OK: $($Consumers.Count) consumer(s) built and passed against d3-format $version ($crateSha) on $pinnedToolchain and $MsrvToolchain."
}
catch {
    Write-Host ''
    Write-Host "FAILED: $($_.Exception.Message)"
    exit 1
}

exit 0

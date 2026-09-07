#Requires -Version 5.1

<#
.SYNOPSIS
    Runs the Rust conformance checks for the d3-format port.

.DESCRIPTION
    This is the single entry point used by the Rust jobs in
    .github/workflows/test.yml. It implements the check sets described in
    sections 6.1 and 6.2 of MIGRATION_TO_RUST.md.

    The script is authored for PowerShell 7 (the `pwsh` shell used by CI on both
    ubuntu-latest and windows-latest) and avoids constructs that behave
    differently on Windows PowerShell 5.1, so it can also be run locally on a
    developer machine that only has 5.1. It contains no Bash syntax; section 6
    forbids embedding Bash in a PowerShell step.

    Every cargo invocation that resolves dependencies passes --locked. The
    migration rests on byte-reproducibility, and a run that silently re-resolves
    is not reproducing the committed state: --locked turns a stale or drifting
    lockfile into an immediate, clearly attributed failure instead of a mystery
    output difference later. The sharpest case is the MSRV job, where cargo 1.80
    ignores resolver.incompatible-rust-versions (section 1) and would reselect a
    clap release it cannot parse, but it is not the only one. `cargo fmt` is the
    single exception, because it does not resolve.

.PARAMETER Profile
    Selects the check set.

    Pinned  The full set from section 6.1: formatting, lints, the four feature
            combinations, the conformance scoreboard guard, benchmark
            compilation, packaging, and the forbidden dependency guard. "Pinned"
            names the check set, not a toolchain: section 6.2 runs this same set
            on current stable as moving compatibility coverage.

    Msrv    The lighter set from section 6.2: the four feature-combination test
            runs only. MSRV coverage answers "does the declared minimum compiler
            still build this crate and produce the same behavior". Formatting and
            Clippy conformance, packaging, dependency-graph policy, and the
            scoreboard guard are properties of the source, the lockfile and the
            generated fixtures rather than of the compiler, so pinning them to
            one toolchain avoids failing the MSRV job for a lint whose wording
            changed between releases and avoids asserting the same
            toolchain-independent count twice.

    Note that `Profile` shadows PowerShell's automatic $PROFILE variable inside
    this script. The name is fixed by section 6.1, which documents the invocation
    as `./ci/check.ps1 -Profile Pinned`, and the script never needs $PROFILE.

.PARAMETER AllowDirty
    Passes --allow-dirty to `cargo package`. `cargo package` refuses to run when
    the crate directory has uncommitted or untracked files, so a local run always
    fails at that step while work is in progress. CI builds a clean checkout and
    must never set this.

.EXAMPLE
    ./ci/check.ps1 -Profile Pinned

.EXAMPLE
    ./ci/check.ps1 -Profile Msrv
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('Pinned', 'Msrv')]
    [string] $Profile,

    [switch] $AllowDirty
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

# Packages that must never appear in the dependency graph. Criterion 0.8.2 pulls
# `alloca`, which builds through `cc`; section 5.1 pins criterion to exactly
# 0.7.0 to avoid that build dependency and this guard is what keeps the pin
# honest against an accidental `cargo update`.
$ForbiddenPackages = @('cc', 'alloca')

function Invoke-Step {
    <#
        Runs a native command and turns a non-zero exit code into a terminating
        error. This check is not optional: a native command's exit code does not
        stop a PowerShell script on its own, and $ErrorActionPreference only
        covers native exit codes on PowerShell 7.4 and later. Without the
        explicit test the script would report success on Windows PowerShell 5.1
        even after `cargo test` failed.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string] $Command,

        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [string[]] $Arguments
    )

    $rendered = ($Command + ' ' + ($Arguments -join ' ')).TrimEnd()
    Write-Host ''
    Write-Host "==> $rendered"

    & $Command @Arguments

    if ($LASTEXITCODE -ne 0) {
        throw "$rendered failed with exit code $LASTEXITCODE"
    }
}

function Write-ToolchainBanner {
    <#
        Prints the compiler actually in use. rust-toolchain.toml pins channel
        1.98.1 and applies to any cargo invocation made from inside rust/, so the
        MSRV and stable jobs rely on the RUSTUP_TOOLCHAIN environment variable to
        override it. That override is invisible in the command line, and a job
        that lost it would silently test 1.98.1 while claiming to test something
        else, so record the resolved versions in the log.
    #>

    $requested = $env:RUSTUP_TOOLCHAIN
    if ([string]::IsNullOrEmpty($requested)) {
        $requested = '(unset; rust-toolchain.toml decides)'
    }

    Write-Host "RUSTUP_TOOLCHAIN: $requested"
    Invoke-Step -Command 'rustc' -Arguments @('--version')
    Invoke-Step -Command 'cargo' -Arguments @('--version')
}

function Assert-NoForbiddenDependencies {
    <#
        Section 6.1's forbidden-dependency guard. `cargo metadata --all-features`
        reports every package the lockfile resolves for the full feature set, so
        a C-backed build dependency cannot hide behind an optional feature.
    #>

    Write-Host ''
    Write-Host '==> cargo metadata --locked --format-version 1 --all-features (forbidden dependency guard)'

    # --locked matters here too: cargo metadata resolves dependencies and rewrites
    # Cargo.lock when it decides the lock is out of date, so without the flag the
    # guard could report a graph that is not the committed one.
    $output = & cargo metadata --locked --format-version 1 --all-features
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata --locked --format-version 1 --all-features failed with exit code $LASTEXITCODE"
    }

    # cargo emits the document on one line, but join defensively: Windows
    # PowerShell 5.1 parses each pipeline item separately and would reject a
    # multi-line document.
    $metadata = ($output -join "`n") | ConvertFrom-Json
    $forbidden = @($metadata.packages | Where-Object { $_.name -in $ForbiddenPackages })

    if ($forbidden.Count -ne 0) {
        # Diagnostics before failing, so the log shows which dependency pulled the
        # package in. Section 6.1 spells this as `cargo tree -i cc`; querying each
        # offender by name keeps the output useful if only `alloca` is present.
        # --all-features matches the graph the guard just inspected, so an offender
        # that enters through an optional feature is still explained. Deliberately
        # no --locked and no exit-code check: this runs only on the way to a
        # failure, and a diagnostic must never mask the error it is explaining.
        foreach ($name in @($forbidden | ForEach-Object { $_.name } | Sort-Object -Unique)) {
            & cargo tree --all-features --invert $name
        }

        $found = @($forbidden |
            ForEach-Object { "$($_.name) $($_.version)" } |
            Sort-Object -Unique) -join ', '
        throw "a forbidden C-backed benchmark dependency entered the graph: $found"
    }

    Write-Host "    clean: none of $($ForbiddenPackages -join ', ') are in the graph"
}

function Assert-ConformanceScoreboardIsStillIgnored {
    <#
        Phase 1 generated one conformance test per JavaScript test block, each one
        #[ignore]d with an unimplemented!() body, plus hand-ported placeholders for
        the assertions that cannot become generated cases. Plain `cargo test`
        therefore prints `test result: ok.` and exits 0 while the port does not
        exist. Every placeholder target carries a non-ignored test whose name says
        so, which a human reading the output cannot miss, but a dashboard that
        surfaces only summary lines and the exit code still can.

        This turns that state into a hard pass/fail. The expected counts are read
        from the generated artifacts rather than written here, so the assertion
        tracks the data instead of drifting from it.
    #>

    Write-Host ''
    Write-Host '==> conformance scoreboard guard (ignored-test inventory)'

    $fixtures = Join-Path 'fixtures' 'd3-format-3.1.2'
    $manifestPath = Join-Path $fixtures 'manifest.json'
    $mapPath = Join-Path $fixtures 'assertion-map.json'

    $expected = [ordered] @{}

    # counts.test_blocks is the provenance record of how many JavaScript test blocks
    # were harvested, and gen-tests.mjs emits exactly one #[ignore]d Rust test per
    # block. assertion-map.json cannot serve here: it names only the generated tests
    # that some assertion site routes to, which is fewer than the blocks that exist.
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $expected['generated'] = [int] $manifest.counts.test_blocks

    # The ported target exists only to receive assertions the map routes to it, and
    # check-coverage.mjs already fails if the map names a test that target does not
    # define, so the distinct test names under rust/tests/ported/ are the recorded
    # size of the target. The map's counts.ported and counts.divergence are
    # assertion-site totals rather than test totals, so they are the wrong fields.
    $map = Get-Content -LiteralPath $mapPath -Raw | ConvertFrom-Json
    $expected['ported'] = @($map.sites |
        ForEach-Object { $_.rust_tests } |
        Where-Object { $_ -like 'rust/tests/ported/*' } |
        Sort-Object -Unique).Count

    foreach ($target in @($expected.Keys)) {
        if ($expected[$target] -le 0) {
            throw "no expected ignored-test count for the '$target' target could be derived from $manifestPath and $mapPath"
        }
    }

    foreach ($target in @($expected.Keys)) {
        # `--list --ignored` is an inventory query: it enumerates the ignored tests
        # without running any of them, so the number cannot be perturbed by a test
        # outcome, and asking per target attributes each count unambiguously.
        # Parsing the `N ignored` field of a whole-suite run would instead mean
        # correlating six summary lines with the `Running ...` lines above them, and
        # would stop reporting a usable number as soon as some test in the target
        # failed -- which is exactly when the scoreboard needs to be trustworthy.
        # The counts do not vary with the feature set; --all-features simply matches
        # the rest of this profile.
        $arguments = @('test', '--locked', '--all-features', '--test', $target, '--', '--list', '--ignored')
        $listed = & cargo @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "cargo $($arguments -join ' ') failed with exit code $LASTEXITCODE"
        }

        # libtest lists one `<path>::<name>: test` line per ignored test, and spells
        # benchmarks `: bench`, which this deliberately does not count.
        $observed = @($listed | Where-Object { $_ -match ': test$' }).Count
        Write-Host "    $target : $observed ignored, recorded $($expected[$target])"

        if ($observed -ne $expected[$target]) {
            throw (@(
                "the conformance scoreboard moved: the '$target' test target has $observed ignored tests, but the generated artifacts record $($expected[$target]).",
                '',
                'This is a checkpoint, not a bug. The number of not-yet-passing conformance',
                "blocks is the migration's progress metric, so it is meant to change only",
                'through an explicit, reviewed regeneration rather than drifting silently.',
                '',
                'If you are implementing Phases 2 to 4 and have just un-ignored tests, this',
                'failure is expected. Regenerate the fixtures with the rust/tools generators so',
                'manifest.json and assertion-map.json record the new state, and review the count',
                'change in the same commit.',
                '',
                'If you did not mean to touch the scoreboard, an #[ignore] attribute or a',
                'generated test has been lost.'
            ) -join [Environment]::NewLine)
        }
    }
}

# Section 6.1 sets working-directory: rust, but anchoring to the script's own
# location makes the script correct from any caller. Cargo also discovers
# .cargo/config.toml by walking up from the working directory, and section 1
# requires the rust/ copy to be the one in effect for crate-directory builds.
$crateRoot = Split-Path -Parent $PSScriptRoot

try {
    Push-Location -LiteralPath $crateRoot
    try {
        Write-Host "d3-format Rust checks"
        Write-Host "  profile:   $Profile"
        Write-Host "  directory: $crateRoot"

        Write-ToolchainBanner

        if ($Profile -eq 'Pinned') {
            # cargo fmt is the one step that gets no --locked: it never resolves
            # dependencies, and the flag is not accepted there.
            Invoke-Step -Command 'cargo' -Arguments @('fmt', '--check')
            Invoke-Step -Command 'cargo' -Arguments @(
                'clippy', '--locked', '--all-targets', '--all-features', '--', '-D', 'warnings')
        }

        # The four feature combinations from section 6.1. Features are additive and
        # must not change formatting results, so every combination runs the tests.
        Invoke-Step -Command 'cargo' -Arguments @('test', '--locked', '--no-default-features')
        Invoke-Step -Command 'cargo' -Arguments @('test', '--locked', '--features', 'locales')
        Invoke-Step -Command 'cargo' -Arguments @('test', '--locked', '--features', 'serde')
        Invoke-Step -Command 'cargo' -Arguments @('test', '--locked', '--all-features')

        if ($Profile -eq 'Pinned') {
            # Runs here because the test targets have just been built, so the
            # inventory queries reuse that build instead of triggering another.
            Assert-ConformanceScoreboardIsStillIgnored

            # Until Phase 5 adds benches/format.rs this compiles Criterion and the
            # implicit library bench target only. It is a dependency-graph and
            # linking check, not benchmark coverage.
            Invoke-Step -Command 'cargo' -Arguments @('bench', '--locked', '--no-run')

            $packageArguments = @('package', '--locked')
            if ($AllowDirty) {
                $packageArguments += '--allow-dirty'
            }
            Invoke-Step -Command 'cargo' -Arguments $packageArguments

            Assert-NoForbiddenDependencies
        }

        Write-Host ''
        Write-Host "OK: profile '$Profile' passed."
    }
    finally {
        Pop-Location
    }
}
catch {
    Write-Host ''
    Write-Host "FAILED: profile '$Profile': $($_.Exception.Message)"
    # Exit explicitly rather than relying on the host to translate a terminating
    # error into a process exit code, which differs between PowerShell versions
    # and between interactive and CI invocation.
    exit 1
}

exit 0

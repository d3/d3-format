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
            combinations, the conformance inventory guard, benchmark compilation
            and a benchmark smoke run, packaging, and the forbidden dependency
            guard. "Pinned" names the check set, not a toolchain: section 6.2 runs
            this same set on current stable as moving compatibility coverage.

    Msrv    The lighter set from section 6.2: the four feature-combination test
            runs only. MSRV coverage answers "does the declared minimum compiler
            still build this crate and produce the same behavior". Formatting and
            Clippy conformance, packaging, dependency-graph policy, and the
            conformance inventory are properties of the source, the lockfile and
            the generated fixtures rather than of the compiler, so pinning them
            to one toolchain avoids failing the MSRV job for a lint whose wording
            changed between releases and avoids asserting the same
            toolchain-independent inventory twice.

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

function Get-ListedTestNames {
    <#
        Names every test libtest would run in one test target, without running any
        of them. `--list` is an inventory query, so the result cannot be perturbed
        by a test outcome, and asking one target at a time attributes each name
        unambiguously -- unlike parsing a whole-suite run, whose summary lines have
        to be matched back to the `Running ...` lines above them and stop being
        usable the moment some test in the target fails.

        The inventory does not vary with the feature set; --all-features simply
        matches the rest of this profile.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]
        [string] $Target,

        [switch] $IgnoredOnly
    )

    $arguments = @('test', '--locked', '--all-features', '--test', $Target, '--', '--list')
    if ($IgnoredOnly) {
        $arguments += '--ignored'
    }

    $listed = & cargo @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($arguments -join ' ') failed with exit code $LASTEXITCODE"
    }

    # libtest prints one `<module path>::<name>: test` line per test and spells
    # benchmarks `: bench`, which this deliberately does not match, then a trailing
    # `N tests, M benchmarks` summary, which it also does not match.
    , @($listed |
        Where-Object { $_ -match ': test$' } |
        ForEach-Object { $_ -replace ': test$', '' })
}

function Assert-ConformanceInventoryMatchesTheArtifacts {
    <#
        Phase 1 generated one conformance test per JavaScript test block and one
        hand-ported test per assertion that cannot become a generated case. A
        target whose tests are all #[ignore]d still prints `test result: ok.` and
        exits 0, so a dashboard that surfaces only summary lines and the exit code
        can read a placeholder suite as working coverage. This makes the suite's
        shape a hard pass/fail instead.

        It gates how many conformance tests EXIST, not how many pass. An earlier
        revision asserted the #[ignore]d count, which was the wrong quantity: how
        many tests pass moves with every phase, so Phase 3 could not both satisfy
        its exit condition (the eleven hand-ported tests pass) and this guard (they
        are ignored), and could not record the transition either, because
        assertion-map.json belongs to Phase 1's generators. How many tests exist is
        a stable property the generators already record. The ignored count is still
        reported, as the migration's progress metric, but nothing gates on it.
    #>

    Write-Host ''
    Write-Host '==> conformance inventory guard'

    $fixtures = Join-Path 'fixtures' 'd3-format-3.1.2'
    $manifestPath = Join-Path $fixtures 'manifest.json'
    $mapPath = Join-Path $fixtures 'assertion-map.json'

    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $map = Get-Content -LiteralPath $mapPath -Raw | ConvertFrom-Json

    # assertion-map.json spells a test `rust/tests/<target>/<module>.rs::<name>`;
    # libtest spells the same test `<module>::<name>`.
    $recorded = @{}
    foreach ($name in @('generated', 'ported')) {
        $prefix = "rust/tests/$name/"
        $recorded[$name] = @($map.sites |
            ForEach-Object { $_.rust_tests } |
            Where-Object { $_ -like "$prefix*" } |
            ForEach-Object { $_.Substring($prefix.Length) -replace '\.rs::', '::' } |
            Sort-Object -Unique)
    }

    $targets = @(
        [pscustomobject] @{
            Name = 'generated'
            # counts.test_blocks is the provenance record of how many JavaScript test
            # blocks were harvested, and gen-tests.mjs emits exactly one Rust test per
            # block. assertion-map.json cannot supply this number: it names only the
            # generated tests some assertion site routes to, currently 155 of 168.
            Expected = [int] $manifest.counts.test_blocks
            ExpectedFrom = "$manifestPath (counts.test_blocks)"
            # The generator controls this spelling: `<source file>_test::t<NNN>_<title>`.
            ConventionPattern = '^[a-z0-9_]+_test::t[0-9]{3}_'
            RecordedIsExhaustive = $false
        },
        [pscustomobject] @{
            Name = 'ported'
            # This target exists only to receive assertions the map routes to it, and
            # check-coverage.mjs already fails if the map names a test the target does
            # not define, so the map records this inventory exhaustively and by name.
            # counts.ported and counts.divergence are assertion-site totals rather
            # than test totals, so they are the wrong fields.
            Expected = $recorded['ported'].Count
            ExpectedFrom = "$mapPath (distinct rust/tests/ported/ test names)"
            # Hand-written names follow no generator convention.
            ConventionPattern = $null
            RecordedIsExhaustive = $true
        }
    )

    foreach ($target in $targets) {
        if ($target.Expected -le 0) {
            throw "no recorded conformance-test count for the '$($target.Name)' target could be derived from $($target.ExpectedFrom)"
        }
    }

    foreach ($target in $targets) {
        $listed = Get-ListedTestNames -Target $target.Name
        $ignored = Get-ListedTestNames -Target $target.Name -IgnoredOnly

        # Conformance tests live in the target's generated or hand-ported submodules;
        # the scaffolding that must not be counted -- the generated inventory check,
        # Phase 3's aggregators, and whatever later phases add -- is declared in the
        # target's own main.rs and so has no module path.
        $conformance = @($listed | Where-Object { $_ -match '::' } | Sort-Object)

        if ($target.ConventionPattern) {
            # Second, independent discriminator. The structural rule above says where a
            # test is declared; this one says what the generator named it. They select
            # the same set today, and requiring them to keep agreeing means neither
            # convention can drift silently: a change turns into this diagnosable
            # failure instead of a quietly wrong count.
            $byConvention = @($listed | Where-Object { $_ -match $target.ConventionPattern } | Sort-Object)
            $structuralOnly = @($conformance | Where-Object { $byConvention -notcontains $_ })
            $conventionOnly = @($byConvention | Where-Object { $conformance -notcontains $_ })

            if ($structuralOnly.Count -ne 0 -or $conventionOnly.Count -ne 0) {
                throw (@(
                    "the two conformance-test discriminators disagree in the '$($target.Name)' target, so its inventory cannot be counted safely.",
                    "  declared in a submodule : $($conformance.Count)",
                    "  named $($target.ConventionPattern) : $($byConvention.Count)",
                    "  submodule only : $(if ($structuralOnly.Count) { $structuralOnly -join ', ' } else { '(none)' })",
                    "  naming only    : $(if ($conventionOnly.Count) { $conventionOnly -join ', ' } else { '(none)' })",
                    '',
                    'One of the two conventions changed. Update the discriminator in ci/check.ps1',
                    'deliberately rather than letting the counted set drift.'
                ) -join [Environment]::NewLine)
            }
        }

        Write-Host "    $($target.Name) : $($conformance.Count) conformance tests defined, recorded $($target.Expected); $($ignored.Count) of $($listed.Count) listed tests still #[ignore]d"

        # Names first: they say which test moved, where a count says only that one did.
        $missing = @($recorded[$target.Name] | Where-Object { $conformance -notcontains $_ })
        if ($missing.Count -ne 0) {
            throw (@(
                "the '$($target.Name)' target no longer defines $($missing.Count) test(s) that $mapPath routes assertions to:",
                ($missing | ForEach-Object { "  $_" })
            ) -join [Environment]::NewLine)
        }

        if ($target.RecordedIsExhaustive) {
            $unrecorded = @($conformance | Where-Object { $recorded[$target.Name] -notcontains $_ })
            if ($unrecorded.Count -ne 0) {
                throw (@(
                    "the '$($target.Name)' target defines $($unrecorded.Count) conformance test(s) that $mapPath does not record:",
                    ($unrecorded | ForEach-Object { "  $_" })
                ) -join [Environment]::NewLine)
            }
        }

        if ($conformance.Count -ne $target.Expected) {
            throw (@(
                "the conformance inventory moved: the '$($target.Name)' target defines $($conformance.Count) conformance tests, but the artifacts record $($target.Expected).",
                '',
                'This gates how many conformance tests exist, not how many pass. A test moving',
                'from #[ignore]d to passing is ordinary phase progress and does not reach here;',
                'the ignored count in the line above is reported for information only.',
                '',
                'So a conformance test was added or removed. If that was deliberate, regenerate',
                'the fixtures with the rust/tools generators so the recorded inventory in',
                "$($target.ExpectedFrom)",
                'matches, and review the change in the same commit.',
                '',
                'If it was not deliberate, a generated or hand-ported test has been lost.'
            ) -join [Environment]::NewLine)
        }
    }

    Write-Host '    the ignored counts are the progress metric and are reported only; this guard gates the inventory'
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
            Assert-ConformanceInventoryMatchesTheArtifacts

            # Section 6.1's benchmark gate. Before Phase 5 this compiled Criterion
            # and the implicit library bench target only, and the profile said so;
            # it now also compiles benches/format.rs, so it is a real check that the
            # benchmark still builds against the API it measures. On a GNU Windows
            # host this is the step that exercises the criterion -> windows-sys
            # raw-dylib link section 2.2 flags.
            Invoke-Step -Command 'cargo' -Arguments @('bench', '--locked', '--no-run')

            # Compiling a benchmark is not running it. Criterion's --test mode runs
            # each benchmark once, with no measurement, which is cheap because the
            # build above is reused -- and it is the only thing that would catch a
            # benchmark that builds and then panics on its first iteration. It is
            # not a performance gate: nothing here compares timings, and a hosted
            # runner's timings would not be worth comparing.
            Invoke-Step -Command 'cargo' -Arguments @('bench', '--locked', '--', '--test')

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

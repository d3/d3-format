#Requires -Version 5.1

<#
.SYNOPSIS
    Runs a named differential corpus and archives its summary as a committed
    artifact.

.DESCRIPTION
    Section 6.3 of MIGRATION_TO_RUST.md ends with an open question, and this script
    is Phase 6's answer to it. The differential runner writes its machine-readable
    summary under rust/target/, which is gitignored, so section 6.3 calls that
    output "a CI artifact, not a committed file" and hands archiving to Phase 6.

    The division of labour is therefore: rust/target/differential/ stays the run's
    own output and stays ephemeral -- nothing here changes where the runner writes,
    and .github/workflows/prerelease-differential.yml still asserts against and
    uploads that file. What Phase 6 adds is the archive: a committed record of one
    release's evidence, at a path a reader can find months later without a CI run
    to download from.

    Like ci/benchmarks/collect-baseline.ps1, every field is read out of the run
    rather than transcribed. The script runs the differential itself, requires the
    summary to be one this run produced rather than one left over from an earlier
    invocation, refuses to archive a run that did not report exact agreement, and
    adds the provenance the summary does not carry: the toolchain, the engine, the
    host, and when it happened.

    Authored for Windows PowerShell 5.1 as well as PowerShell 7, like ci/check.ps1,
    and it contains no Bash syntax.

.PARAMETER Corpus
    The named corpus from section 6.3. `ci` is the per-PR gate: 200,000 drawn
    formatter cases on top of the fixed structured sections, in debug so that
    Rust's overflow checks stay live. `prerelease` is the scheduled gate: the
    complete structured corpora plus eleven million drawn cases, in release,
    because that is the only way the run is affordable.

.PARAMETER Seed
    Corpus seed. The default reproduces the archived runs.

.PARAMETER Cases
    Drawn formatter cases. The defaults match .github/workflows/test.yml for `ci`
    and .github/workflows/prerelease-differential.yml for `prerelease`; roughly one
    drawn specifier in fifteen is type `c` and is a policy check rather than an
    output comparison, which is the headroom in the pre-release default.

.PARAMETER OutputPath
    Where to write the archive. Defaults to ci/parity/<corpus>-differential.json.

.EXAMPLE
    ./ci/parity/collect-differential.ps1 -Corpus ci

.EXAMPLE
    ./ci/parity/collect-differential.ps1 -Corpus prerelease
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('ci', 'prerelease')]
    [string] $Corpus,

    [int] $Seed = 1,

    [int] $Cases,

    [string] $OutputPath
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

# Per-corpus defaults, kept next to each other so the two cannot drift apart
# silently. --release and the larger heap belong to the pre-release run only.
$corpusDefaults = @{
    ci = @{ Cases = 200000; Release = $false; HeapMb = 0 }
    prerelease = @{ Cases = 11000000; Release = $true; HeapMb = 8192 }
}
$defaults = $corpusDefaults[$Corpus]
if (-not $PSBoundParameters.ContainsKey('Cases')) {
    $Cases = $defaults.Cases
}

function Get-CommandOutput {
    <#
        As ci/benchmarks/collect-baseline.ps1: a native command's exit code does not
        stop a PowerShell script on its own, and $ErrorActionPreference does not
        cover native exit codes on Windows PowerShell 5.1.
    #>
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

function Get-HostDescription {
    <#
        Best effort, for the same reason the benchmark collector's is: the CIM
        classes exist only on Windows, and an archive that failed to generate on
        Ubuntu because it could not name the CPU would be worse than a coarser one.
        Wall-clock duration is in the summary, so the host is worth recording even
        though this artifact is a correctness record rather than a timing one.
    #>

    $operatingSystem = [Environment]::OSVersion.VersionString
    $processor = $env:PROCESSOR_IDENTIFIER

    try {
        $cimOs = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
        $operatingSystem = "$($cimOs.Caption) $($cimOs.Version)"
        $cimCpu = @(Get-CimInstance -ClassName Win32_Processor -ErrorAction Stop)[0]
        $processor = $cimCpu.Name
    }
    catch {
        Write-Host "    (no CIM host details available: $($_.Exception.Message))"
    }

    [ordered] @{
        os = "$operatingSystem".Trim()
        cpu = "$processor".Trim()
        logical_cores = [Environment]::ProcessorCount
        architecture = "$env:PROCESSOR_ARCHITECTURE"
    }
}

$crateRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$repositoryRoot = Split-Path -Parent $crateRoot

if ([string]::IsNullOrEmpty($OutputPath)) {
    $OutputPath = Join-Path $PSScriptRoot "$Corpus-differential.json"
}
# Made absolute before Push-Location. [System.IO.File]::WriteAllText resolves a
# relative path against the *process* working directory, which Push-Location does not
# move, so a caller-supplied relative path would otherwise land somewhere else.
$OutputPath = [System.IO.Path]::GetFullPath(
    [System.IO.Path]::Combine((Get-Location -PSProvider FileSystem).ProviderPath, $OutputPath))

# differential.mjs runs from the repository root: it reads the JavaScript oracle
# out of src/ and passes --manifest-path rust/Cargo.toml to cargo itself.
Push-Location -LiteralPath $repositoryRoot
try {
    $summaryPath = "rust/target/differential/$Corpus-summary.json"

    Write-Host "d3-format differential archive"
    Write-Host "  corpus:  $Corpus"
    Write-Host "  seed:    $Seed"
    Write-Host "  cases:   $Cases drawn"
    Write-Host "  summary: $summaryPath"
    Write-Host "  output:  $OutputPath"

    # A summary left behind by an earlier invocation would otherwise be
    # indistinguishable from this run's if the run died before writing one.
    if (Test-Path -LiteralPath $summaryPath) {
        Remove-Item -LiteralPath $summaryPath -Force
    }

    $nodeArguments = @()
    if ($defaults.HeapMb -gt 0) {
        $nodeArguments += "--max-old-space-size=$($defaults.HeapMb)"
    }
    $nodeArguments += @(
        'rust/tools/differential.mjs',
        '--corpus', $Corpus,
        '--seed', "$Seed",
        '--cases', "$Cases",
        '--summary', $summaryPath
    )
    if ($defaults.Release) {
        $nodeArguments += '--release'
    }

    Write-Host ''
    Write-Host "==> node $($nodeArguments -join ' ')"
    $startedUtc = (Get-Date).ToUniversalTime()

    & node @nodeArguments
    if ($LASTEXITCODE -ne 0) {
        throw "node $($nodeArguments -join ' ') failed with exit code $LASTEXITCODE"
    }

    $finishedUtc = (Get-Date).ToUniversalTime()

    if (-not (Test-Path -LiteralPath $summaryPath)) {
        throw "the run exited zero but wrote no summary to $summaryPath"
    }
    # The run just deleted and rewrote it, so this can only fail if something else
    # supplied the file. Belt and braces, in the same spirit as the benchmark
    # collector's stale-estimate check.
    $written = (Get-Item -LiteralPath $summaryPath).LastWriteTimeUtc
    if ($written -lt $startedUtc.AddMinutes(-1)) {
        throw "$summaryPath was written at $written, before this run started at $startedUtc"
    }

    Write-Host ''
    Write-Host '==> ci/parity/assert-agreement.ps1'
    & (Join-Path $PSScriptRoot 'assert-agreement.ps1') -SummaryPath $summaryPath

    $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json

    $archive = [ordered] @{
        schema = 'd3-format/differential-archive/1'
        generated_utc = $finishedUtc.ToString('yyyy-MM-ddTHH:mm:ssZ')
        command = 'node ' + ($nodeArguments -join ' ')
        regenerate_with = "pwsh ci/parity/collect-differential.ps1 -Corpus $Corpus  (run from rust/)"
        provenance = [ordered] @{
            # The commit the evidence describes. The archive is only about the tree
            # it was produced from, and an archive whose commit a reader cannot
            # recover is not evidence of anything.
            git_commit = (Get-CommandOutput -Command 'git' -Arguments @('rev-parse', 'HEAD'))[0]
            # An archive that names a commit it was not actually produced from is
            # worse than one that names none, and the archive itself is an
            # uncommitted file at the moment it is written, so the honest answer is
            # to say whether anything else was uncommitted too.
            git_worktree_dirty = @(Get-CommandOutput -Command 'git' -Arguments @(
                    'status', '--porcelain', '--untracked-files=normal')).Count -gt 0
            node = (Get-CommandOutput -Command 'node' -Arguments @('--version'))[0]
            rustc = (Get-CommandOutput -Command 'rustc' -Arguments @('--version'))[0]
            cargo = (Get-CommandOutput -Command 'cargo' -Arguments @('--version'))[0]
            cargo_profile = if ($defaults.Release) { 'release' } else { 'dev' }
            host = Get-HostDescription
            elapsed_seconds = [math]::Round(($finishedUtc - $startedUtc).TotalSeconds, 1)
        }
        summary = $summary
        # Written without apostrophes on purpose: PowerShell 5.1 escapes one as
        # \u0027, and these exist to be read.
        notes = @(
            'The summary block below is the differential runners own output, copied verbatim from the run this file records. It is not a transcription: nothing in it was typed by hand.',
            'The debug profile keeps Rusts integer overflow checks and debug assertions live, which is the stronger reading of "no accepted input panics". The pre-release corpus trades them for throughput because it is the only way ten million comparisons are affordable; the ci corpus keeps them.',
            'Roughly one drawn specifier in fifteen is type c, which is an input-policy check rather than an output comparison, so formatter.cases and formatter.output_comparisons are different numbers on purpose.',
            'A locale drawing zero cases would be a gate failure rather than a pass. locale_coverage.floor_applied says whether the per-locale floor was in force; below about 200 draws per locale it is not, and the runner says so rather than asserting nothing.',
            'This archive is a record, not a gate. No CI job reads it. The executable gates are ci/parity/assert-agreement.ps1, the parity job in .github/workflows/test.yml and the assertion step in .github/workflows/prerelease-differential.yml.'
        )
    }

    # Compact, and LF with no BOM, for the same reasons ci/benchmarks/baseline.json
    # is: section 1s .gitattributes rule makes rust/** LF and the byte-comparison
    # gates depend on it, and PowerShell 5.1s pretty printer wraps arrays in a shape
    # that changes between PowerShell versions.
    $json = $archive | ConvertTo-Json -Depth 24 -Compress
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    $text = ($json -replace "`r`n", "`n").TrimEnd("`n") + "`n"
    [System.IO.File]::WriteAllText($OutputPath, $text, $utf8NoBom)

    Write-Host ''
    Write-Host "OK: wrote $OutputPath ($([math]::Round($text.Length / 1kb, 1)) KiB)"
}
finally {
    Pop-Location
}

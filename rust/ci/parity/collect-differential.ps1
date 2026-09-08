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

.PARAMETER AllowDirty
    Collects from a working tree that does not match HEAD.

    Without it the collector refuses a dirty tree, in the same shape and for the same
    reason as ci/package/collect-listing.ps1: the archive names the commit it
    measured, and naming a commit is only worth something if a reader can check that
    commit out and reproduce the run. A differential taken over uncommitted work
    describes a tree that exists nowhere in history, which is the one thing an
    agreement record cannot be.

    The switch exists for local experimentation and marks the artifact:
    provenance.allow_dirty is true and provenance.git_worktree_dirty records what the
    tree actually was. An archive with either set is not release evidence.

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

    [switch] $AllowDirty,

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

function Get-WorktreeChanges {
    <#
        Every uncommitted path in the repository, tracked or not, one per output
        object. Untracked counts: a file HEAD does not contain is as much a
        difference from it as a modified one, and `cargo package` treats the two the
        same way.

        git is invoked here rather than through Get-CommandOutput deliberately. That
        helper returns the whole output as one array-valued object -- which is what
        its other callers want, since they index [0] or test .Count -gt 0, but a
        caller that needs the paths themselves gets an array holding an array and
        renders it as "System.Object[]". Normalizing through @() and dropping blank
        lines makes the shape the same for none, one and many.

        Duplicated verbatim in ci/package/collect-listing.ps1 and
        ci/benchmarks/collect-baseline.ps1, as Get-CommandOutput already is, so each
        collector stays runnable on its own.
    #>

    $status = & git 'status' '--porcelain' '--untracked-files=normal'
    if ($LASTEXITCODE -ne 0) {
        throw "git status --porcelain --untracked-files=normal failed with exit code $LASTEXITCODE"
    }
    @($status) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
}

function Assert-CollectedFromACommit {
    <#
        The clean-tree precondition every collector applies before it measures
        anything and again before it writes the result.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [AllowEmptyCollection()] [string[]] $Changes,
        [Parameter(Mandatory = $true)] [string] $When
    )

    if ($Changes.Count -eq 0) {
        return
    }

    # Built by appending rather than as one nested literal: PowerShell does not
    # flatten an array written inside an array literal, and -join would render the
    # inner one as "System.Object[]".
    # ${...} as in ci/parity/assert-agreement.ps1: PowerShell reads `$When:` as a
    # drive-qualified variable and refuses to parse the file at all.
    $message = @("the working tree has $($Changes.Count) uncommitted path(s) ${When}:")
    $message += @($Changes | Select-Object -First 20 | ForEach-Object { "  $_" })
    if ($Changes.Count -gt 20) {
        $message += "  ... and $($Changes.Count - 20) more"
    }
    $message += @(
        '',
        'Release evidence names the commit it measured, and the only reason to name one',
        'is so a reader can check it out and reproduce the result. This tree is that',
        'commit plus changes no commit holds, so the artifact would point at nothing.',
        'Commit or stash the work and rerun.',
        '',
        '-AllowDirty overrides this for local experimentation and marks the artifact,',
        'which is then a note to yourself rather than release evidence.'
    )

    throw ($message -join [Environment]::NewLine)
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

    # Read before the run and re-read after it, so the archive cannot name a commit
    # that was not HEAD for the whole collection.
    $commit = (Get-CommandOutput -Command 'git' -Arguments @('rev-parse', 'HEAD'))[0]

    Write-Host "d3-format differential archive"
    Write-Host "  corpus:  $Corpus"
    Write-Host "  seed:    $Seed"
    Write-Host "  cases:   $Cases drawn"
    Write-Host "  commit:  $commit"
    Write-Host "  summary: $summaryPath"
    Write-Host "  output:  $OutputPath"

    # Checked before the run rather than after it. The pre-release corpus takes
    # roughly twelve minutes, and a precondition that reports after the work is a
    # precondition nobody will keep.
    if (-not $AllowDirty) {
        Assert-CollectedFromACommit -Changes @(Get-WorktreeChanges) -When 'before the collection started'
    }

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

    # Re-checked now rather than only at the start: this is the value the archive
    # records, and recording it after asserting it is what makes git_worktree_dirty
    # false an enforced property of the whole run instead of a claim about its first
    # second.
    $changes = @(Get-WorktreeChanges)
    if (-not $AllowDirty) {
        Assert-CollectedFromACommit -Changes $changes -When 'after the collection finished'
    }
    $commitAfter = (Get-CommandOutput -Command 'git' -Arguments @('rev-parse', 'HEAD'))[0]
    if ($commitAfter -ne $commit) {
        throw "HEAD moved from $commit to $commitAfter during the collection, so this archive measures neither"
    }

    $archive = [ordered] @{
        schema = 'd3-format/differential-archive/1'
        generated_utc = $finishedUtc.ToString('yyyy-MM-ddTHH:mm:ssZ')
        command = 'node ' + ($nodeArguments -join ' ')
        regenerate_with = "pwsh ci/parity/collect-differential.ps1 -Corpus $Corpus  (run from rust/)"
        provenance = [ordered] @{
            # The commit the evidence describes. The archive is only about the tree
            # it was produced from, and an archive whose commit a reader cannot
            # recover is not evidence of anything.
            git_commit = $commit
            # ...which is why this must be false. An archive naming a commit whose
            # tree it did not measure is worse than one naming none, so the collector
            # refuses a dirty tree and this field records the fact it enforced.
            git_worktree_dirty = $changes.Count -gt 0
            allow_dirty = [bool] $AllowDirty
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
            'provenance.git_commit names the commit whose tree was measured. This artifact records that measurement and therefore lands in a later commit, which is expected rather than a defect. What makes it evidence is provenance.git_worktree_dirty being false: the tree measured was exactly the named commit, so a reader can check that commit out and reproduce this. The collector refuses a dirty tree, so the false value is enforced rather than asserted; -AllowDirty waives the refusal and says so in provenance.allow_dirty.',
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

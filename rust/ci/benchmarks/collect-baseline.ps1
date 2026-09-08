#Requires -Version 5.1

<#
.SYNOPSIS
    Regenerates ci/benchmarks/baseline.json from a benchmark run.

.DESCRIPTION
    MIGRATION_TO_RUST.md section 7 makes a benchmark baseline a Phase 5
    deliverable and section 9 requires the artifact to be archived. Criterion
    writes its own results under target/criterion, and target/ is gitignored, so
    criterion's output is not an archive: this script is what turns a run into one.

    Everything in baseline.json is read out of the run rather than transcribed.
    The timings come from criterion's estimates.json files, the allocation profile
    comes from the machine-readable block benches/format.rs prints to stderr, and
    the provenance comes from cargo, rustc and the host. Nothing is typed in by
    hand, which is the only way a regenerated artifact can be compared with the
    committed one and mean anything.

    The inventory is checked, not assumed. The benchmark target is asked for its
    list of ids first, and the script fails if the run did not produce an estimate
    for every one of them. That is what keeps a filtered run, or a stale directory
    left behind by an earlier case table, from silently becoming the archive.

    Authored for Windows PowerShell 5.1 as well as PowerShell 7, like ci/check.ps1,
    and it contains no Bash syntax.

.PARAMETER AllowDirty
    Collects from a working tree that does not match HEAD.

    Without it the collector refuses a dirty tree, in the same shape and for the same
    reason as ci/package/collect-listing.ps1: timings are only interpretable against
    the tree that produced them, and a tree that exists nowhere in history is one
    nobody can build again to compare against.

    The switch exists for local experimentation and marks the artifact:
    provenance.allow_dirty is true and provenance.git_worktree_dirty records what the
    tree actually was. An archive with either set is not release evidence.

.PARAMETER OutputPath
    Where to write the baseline. Defaults to baseline.json next to this script.
    The default is resolved in the body rather than here because Windows
    PowerShell 5.1 has not populated $PSScriptRoot yet while it binds parameters.

.EXAMPLE
    ./ci/benchmarks/collect-baseline.ps1
#>

[CmdletBinding()]
param(
    [switch] $AllowDirty,

    [string] $OutputPath
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrEmpty($OutputPath)) {
    $OutputPath = Join-Path $PSScriptRoot 'baseline.json'
}

$BenchTarget = 'format'
$BenchArguments = @('bench', '--locked', '--bench', $BenchTarget)
$ProfileBegin = '#!d3-format-allocation-profile-begin'
$ProfileEnd = '#!d3-format-allocation-profile-end'

function Invoke-Native {
    <#
        As ci/check.ps1's Invoke-Step: a native command's exit code does not stop a
        PowerShell script on its own, and $ErrorActionPreference does not cover
        native exit codes on Windows PowerShell 5.1.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)] [string] $Command,
        [Parameter(Mandatory = $true)] [AllowEmptyCollection()] [string[]] $Arguments
    )

    $rendered = ($Command + ' ' + ($Arguments -join ' ')).TrimEnd()
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$rendered failed with exit code $LASTEXITCODE"
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
        ci/parity/collect-differential.ps1, as Get-CommandOutput already is, so each
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
        Best effort by design. The CIM classes exist only on Windows, and a
        baseline that failed to generate on Ubuntu because it could not name the
        CPU would be worse than one that records a coarser string.
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

function Get-CompetingProcesses {
    <#
        A wall-clock benchmark taken on a working developer machine is worth
        exactly as much as the reader knows about what else the machine was doing,
        so the artifact records it instead of implying an idle host. cargo and
        rustc are the ones that matter -- a parallel build saturates every core --
        while node is usually just the editor and is counted for context.
    #>

    $running = @(Get-Process -Name 'cargo', 'rustc', 'node' -ErrorAction SilentlyContinue |
        Group-Object -Property ProcessName |
        ForEach-Object { "$($_.Name) x$($_.Count)" } |
        Sort-Object)

    if ($running.Count -eq 0) { 'none' } else { $running -join ', ' }
}

$crateRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

Push-Location -LiteralPath $crateRoot
try {
    # Read before the run and re-read after it, so the baseline cannot name a commit
    # that was not HEAD for the whole collection.
    $commit = (Get-CommandOutput -Command 'git' -Arguments @('rev-parse', 'HEAD'))[0]

    Write-Host "d3-format benchmark baseline"
    Write-Host "  directory: $crateRoot"
    Write-Host "  commit:    $commit"
    Write-Host "  output:    $OutputPath"

    # Checked before the run rather than after it. The full benchmark set takes
    # several minutes, and a precondition that reports after the work is a
    # precondition nobody will keep.
    if (-not $AllowDirty) {
        Assert-CollectedFromACommit -Changes @(Get-WorktreeChanges) -When 'before the collection started'
    }

    # The inventory the run must produce, taken from the target itself. --list
    # builds the benchmark but times nothing, so this costs a second or two and
    # removes every guess about which ids are supposed to exist.
    Write-Host ''
    Write-Host "==> cargo $($BenchArguments -join ' ') -- --list"
    $listed = Get-CommandOutput -Command 'cargo' -Arguments ($BenchArguments + @('--', '--list'))
    $expectedIds = @($listed |
        ForEach-Object { if ($_ -match '^(?<id>.+): benchmark$') { $Matches['id'] } })
    if ($expectedIds.Count -eq 0) {
        throw 'the benchmark target listed no benchmarks'
    }
    Write-Host "    $($expectedIds.Count) benchmarks defined"

    $logPath = Join-Path $crateRoot 'target\benchmark-baseline.log'
    $logDirectory = Split-Path -Parent $logPath
    if (-not (Test-Path -LiteralPath $logDirectory)) {
        New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
    }

    Write-Host ''
    Write-Host "==> cargo $($BenchArguments -join ' ')"
    $startedUtc = (Get-Date).ToUniversalTime()
    $competingAtStart = Get-CompetingProcesses
    Write-Host "    other toolchain processes at start: $competingAtStart"

    # Both streams are captured: the allocation profile is on stderr and
    # criterion's report is on stdout, the archive needs the first and the operator
    # wants to watch the second.
    #
    # Each record is turned into a string before anything writes it down, which is
    # load-bearing rather than tidy. Tee-Object and Out-File *format* their input,
    # and formatting wraps at the console width -- which cuts the profile's JSON
    # lines in half and leaves the archive unparseable. Converting first means the
    # log receives the lines the process actually emitted.
    #
    # $ErrorActionPreference is relaxed for the call because PowerShell surfaces a
    # native command's stderr as error records and cargo writes its ordinary
    # progress there.
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & cargo @BenchArguments 2>&1 |
            ForEach-Object {
                $line = "$_"
                Write-Host $line
                $line
            } |
            Set-Content -LiteralPath $logPath
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($BenchArguments -join ' ') failed with exit code $LASTEXITCODE"
    }

    $finishedUtc = (Get-Date).ToUniversalTime()
    $competingAtEnd = Get-CompetingProcesses

    # The allocation profile, straight out of the run's own output. Trimmed
    # comparisons and a leading-brace test rather than exact equality: PowerShell
    # surfaces native stderr as error records and renders at least the first of
    # them with its own decoration, so the block is located by content instead of
    # by assuming the stream survived untouched.
    $log = @(Get-Content -LiteralPath $logPath)
    $begin = -1
    $end = -1
    for ($index = 0; $index -lt $log.Count; $index++) {
        $line = "$($log[$index])".Trim()
        if ($line -eq $ProfileBegin) { $begin = $index }
        elseif ($line -eq $ProfileEnd) { $end = $index }
    }
    if ($begin -lt 0 -or $end -lt 0 -or $end -le $begin) {
        throw "the run's output does not contain an allocation profile between $ProfileBegin and $ProfileEnd"
    }

    $profileRecords = @()
    $benchDefaults = $null
    foreach ($raw in $log[($begin + 1)..($end - 1)]) {
        $line = "$raw".Trim()
        if (-not $line.StartsWith('{')) { continue }
        try {
            $record = $line | ConvertFrom-Json
        }
        catch {
            throw "an allocation profile line is not valid JSON: $line"
        }
        if ($record.kind -eq 'defaults') {
            $benchDefaults = $record
        }
        else {
            $profileRecords += $record
        }
    }
    if ($profileRecords.Count -eq 0) {
        throw 'the allocation profile block is empty'
    }
    Write-Host "    allocation profile: $($profileRecords.Count) records"

    # Criterion's own estimates. benchmark.json carries the full id, so the
    # directory names criterion derives -- lowercased and punctuation-stripped on
    # Windows -- never have to be parsed or reversed.
    $criterionRoot = Join-Path $crateRoot 'target\criterion'
    if (-not (Test-Path -LiteralPath $criterionRoot)) {
        throw "criterion wrote no output under $criterionRoot"
    }

    $measured = @{}
    foreach ($file in Get-ChildItem -LiteralPath $criterionRoot -Recurse -Filter 'benchmark.json') {
        if ($file.Directory.Name -ne 'new') { continue }
        $estimatesPath = Join-Path $file.DirectoryName 'estimates.json'
        if (-not (Test-Path -LiteralPath $estimatesPath)) { continue }
        $identifier = (Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json)
        $measured[$identifier.full_id] = [pscustomobject] @{
            Identifier = $identifier
            Estimates = (Get-Content -LiteralPath $estimatesPath -Raw | ConvertFrom-Json)
            Written = (Get-Item -LiteralPath $estimatesPath).LastWriteTimeUtc
        }
    }

    $missing = @($expectedIds | Where-Object { -not $measured.ContainsKey($_) })
    if ($missing.Count -ne 0) {
        # Built by appending rather than as one nested literal, as
        # Assert-CollectedFromACommit above builds its message: PowerShell does not
        # flatten an array written inside an array literal, so -join would render the
        # inner one as "System.Object[]" and drop the very ids this message exists to
        # name -- the first time it ever fired.
        $message = @("the run produced no estimate for $($missing.Count) of the $($expectedIds.Count) benchmarks defined:")
        $message += @($missing | ForEach-Object { "  $_" })
        throw ($message -join [Environment]::NewLine)
    }

    # An estimate older than this run is a leftover from an earlier case table, and
    # archiving it would date-stamp someone else's measurement as this one.
    $stale = @($expectedIds | Where-Object { $measured[$_].Written -lt $startedUtc.AddMinutes(-1) })
    if ($stale.Count -ne 0) {
        $message = @('these benchmarks were not re-measured by this run:')
        $message += @($stale | ForEach-Object { "  $_" })
        throw ($message -join [Environment]::NewLine)
    }

    $entries = @()
    foreach ($id in ($expectedIds | Sort-Object)) {
        $estimates = $measured[$id].Estimates
        $identifier = $measured[$id].Identifier
        $entries += [ordered] @{
            id = $id
            group = $identifier.group_id
            case = $identifier.function_id
            mean_ns = [math]::Round($estimates.mean.point_estimate, 4)
            mean_ci_ns = [ordered] @{
                lower = [math]::Round($estimates.mean.confidence_interval.lower_bound, 4)
                upper = [math]::Round($estimates.mean.confidence_interval.upper_bound, 4)
                confidence_level = $estimates.mean.confidence_interval.confidence_level
            }
            median_ns = [math]::Round($estimates.median.point_estimate, 4)
            std_dev_ns = [math]::Round($estimates.std_dev.point_estimate, 4)
        }
    }
    Write-Host "    timings: $($entries.Count) benchmarks"

    $metadata = (Get-CommandOutput -Command 'cargo' -Arguments @(
            'metadata', '--locked', '--format-version', '1', '--all-features')) -join "`n" |
        ConvertFrom-Json
    $crate = @($metadata.packages | Where-Object { $_.name -eq 'd3-format' })[0]
    $criterion = @($metadata.packages | Where-Object { $_.name -eq 'criterion' })[0]

    # Re-checked now rather than only at the start: this is the value the baseline
    # records, and recording it after asserting it is what makes git_worktree_dirty
    # false an enforced property of the whole run instead of a claim about its first
    # second.
    $changes = @(Get-WorktreeChanges)
    if (-not $AllowDirty) {
        Assert-CollectedFromACommit -Changes $changes -When 'after the collection finished'
    }
    $commitAfter = (Get-CommandOutput -Command 'git' -Arguments @('rev-parse', 'HEAD'))[0]
    if ($commitAfter -ne $commit) {
        throw "HEAD moved from $commit to $commitAfter during the collection, so this baseline measures neither"
    }

    $baseline = [ordered] @{
        schema = 'd3-format/benchmark-baseline/1'
        generated_utc = $finishedUtc.ToString('yyyy-MM-ddTHH:mm:ssZ')
        command = 'cargo ' + ($BenchArguments -join ' ')
        regenerate_with = 'pwsh ci/benchmarks/collect-baseline.ps1  (run from rust/)'
        provenance = [ordered] @{
            crate = [ordered] @{ name = $crate.name; version = $crate.version }
            # Added in Phase 6, to match ci/parity/ and ci/package/. Timings are only
            # interpretable against the tree that produced them, and a version alone
            # does not identify a tree: an archived baseline whose commit a reader
            # cannot recover is a set of numbers about nothing in particular.
            git_commit = $commit
            # ...which is why this must be false. The collector refuses a dirty tree,
            # so this field records the fact it enforced rather than reporting one it
            # was willing to tolerate.
            git_worktree_dirty = $changes.Count -gt 0
            allow_dirty = [bool] $AllowDirty
            criterion = $criterion.version
            rustc = (Get-CommandOutput -Command 'rustc' -Arguments @('--version'))[0]
            cargo = (Get-CommandOutput -Command 'cargo' -Arguments @('--version'))[0]
            cargo_profile = 'bench'
            host = Get-HostDescription
            competing_processes = [ordered] @{
                note = 'cargo, rustc and node processes seen on the host; node is normally the editor, a concurrent cargo or rustc is a parallel build competing for every core'
                at_start = $competingAtStart
                at_end = $competingAtEnd
            }
            elapsed_seconds = [math]::Round(($finishedUtc - $startedUtc).TotalSeconds, 1)
            bench_defaults = $benchDefaults
        }
        timings = [ordered] @{
            unit = 'nanoseconds per iteration'
            source = "criterion estimates.json, target/criterion/**/new/, read by $($MyInvocation.MyCommand.Name)"
            entries = $entries
        }
        allocation_profile = [ordered] @{
            unit = 'GlobalAlloc calls and bytes for a single call'
            source = 'the machine-readable block benches/format.rs prints to stderr'
            entries = $profileRecords
        }
        # Written without apostrophes on purpose: PowerShell 5.1 escapes one as
        # \u0027, and this field exists to be read.
        notes = @(
            'Machine-specific and not a regression gate. No CI job reads this file; it is the archived observation MIGRATION_TO_RUST.md section 9 asks for, taken on one developer host with whatever else that host was doing at the time.',
            'provenance.git_commit names the commit whose tree was measured. This artifact records that measurement and therefore lands in a later commit, which is expected rather than a defect. What makes it evidence is provenance.git_worktree_dirty being false: the tree measured was exactly the named commit, so a reader can check that commit out and reproduce this. The collector refuses a dirty tree, so the false value is enforced rather than asserted; -AllowDirty waives the refusal and says so in provenance.allow_dirty.',
            'The difference between the owned_output and reused_output groups is one allocation and one deallocation per call, and on this host that is close to the run-to-run spread of an individual case. Read the direction across the group rather than any single delta, and expect the effect to clear the spread only where the pair is a large share of the call: the type c text case, which does no numeric conversion at all, and the kilobyte binary radix case. The exact form of the comparison is in the allocation profile below, where the reused path makes exactly one fewer allocation in every case measured.',
            'The allocation numbers are exact GlobalAlloc call counts for one call, not statistics. They support "this case made N allocations on this build" and nothing stronger. They do not establish that any path is allocation-free, they say nothing about peak resident memory or fragmentation, and another allocator, optimization level or toolchain may produce different counts for the same code.',
            'Every reused-output measurement ran against a String reserved to the exact output length outside the timed loop, and the benchmark asserts that the capacity was unchanged afterwards, so a reallocation counted on a reused row grew a buffer inside the conversion rather than the output buffer supplied by the caller.',
            'The counting allocator is installed for the whole process. Recording is off during the timed portion, which costs one relaxed atomic load per allocation, so these times are not exactly those of a build without the instrumentation.'
        )
    }

    # Compact rather than pretty: the artifact is read by tools and diffed by
    # humans, and PowerShell 5.1's pretty printer indents with four spaces and
    # wraps arrays in a shape that changes between PowerShell versions -- which
    # would make the file's formatting depend on who regenerated it.
    $json = $baseline | ConvertTo-Json -Depth 12 -Compress

    # Section 1's .gitattributes rule makes rust/** LF, and the byte-identity gates
    # depend on it, so the file is written LF and UTF-8 without a BOM whatever the
    # host's conventions are.
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    $text = ($json -replace "`r`n", "`n").TrimEnd("`n") + "`n"
    [System.IO.File]::WriteAllText($OutputPath, $text, $utf8NoBom)

    Write-Host ''
    Write-Host "OK: wrote $OutputPath ($([math]::Round($text.Length / 1kb, 1)) KiB)"
}
finally {
    Pop-Location
}

' A small test kit for the plain BrightScript of components/lib, run on a
' computer by scripts/test.mjs (brs-node). Each test is a sub named test_…
' in a tests/*.test.brs file; the runner calls tkRun for each.

function tkState() as object
    globals = GetGlobalAA()
    if globals.tkState = invalid then globals.tkState = { passed: 0, failed: 0, errors: [] }
    return globals.tkState
end function

sub tkRun(name as string, test as function)
    state = tkState()
    state.errors = []
    ' Each test starts in English, with the package's words.
    i18nUse("en")
    GetGlobalAA().i18nCache = invalid
    try
        test()
    catch e
        where = []
        if GetInterface(e.backtrace, "ifArray") <> invalid
            for each frame in e.backtrace
                where.Push(frame.function + " (" + frame.filename + ":" + Str(frame.line_number).Trim() + ")")
            end for
        end if
        state.errors.Push("crashed: " + e.message + " " + where.Join(" < "))
    end try
    if state.errors.Count() = 0
        state.passed = state.passed + 1
        print "ok " + name
    else
        state.failed = state.failed + 1
        print "not ok " + name
        for each problem in state.errors
            print "    " + problem
        end for
    end if
end sub

sub tkFinish()
    state = tkState()
    print "tests: " + state.passed.ToStr() + " passed, " + state.failed.ToStr() + " failed"
end sub

sub tkFail(message as string)
    tkState().errors.Push(message)
end sub

sub tkEqual(actual as dynamic, expected as dynamic, what = "" as string)
    if not tkSame(actual, expected) then tkFail(tkWhat(what) + "expected " + tkShow(expected) + ", got " + tkShow(actual))
end sub

sub tkTrue(value as dynamic, what = "" as string)
    if GetInterface(value, "ifBoolean") = invalid or not value then tkFail(tkWhat(what) + "expected true, got " + tkShow(value))
end sub

sub tkFalse(value as dynamic, what = "" as string)
    if GetInterface(value, "ifBoolean") = invalid or value then tkFail(tkWhat(what) + "expected false, got " + tkShow(value))
end sub

sub tkInvalid(value as dynamic, what = "" as string)
    if value <> invalid then tkFail(tkWhat(what) + "expected invalid, got " + tkShow(value))
end sub

function tkWhat(what as string) as string
    if what = "" then return ""
    return what + ": "
end function

function tkIsNumber(value as dynamic) as boolean
    for each name in ["ifInt", "ifLongInt", "ifFloat", "ifDouble"]
        if GetInterface(value, name) <> invalid then return true
    end for
    return false
end function

' Deep equality: numbers by value whatever their type, arrays in order,
' associative arrays by their keys (in any case) and values.
function tkSame(a as dynamic, b as dynamic) as boolean
    if a = invalid or b = invalid then return a = invalid and b = invalid
    if tkIsNumber(a) or tkIsNumber(b) then return tkIsNumber(a) and tkIsNumber(b) and a = b
    if GetInterface(a, "ifString") <> invalid or GetInterface(b, "ifString") <> invalid
        return GetInterface(a, "ifString") <> invalid and GetInterface(b, "ifString") <> invalid and a = b
    end if
    if GetInterface(a, "ifBoolean") <> invalid or GetInterface(b, "ifBoolean") <> invalid
        return GetInterface(a, "ifBoolean") <> invalid and GetInterface(b, "ifBoolean") <> invalid and a = b
    end if
    if GetInterface(a, "ifArray") <> invalid and GetInterface(a, "ifAssociativeArray") = invalid
        if GetInterface(b, "ifArray") = invalid or GetInterface(b, "ifAssociativeArray") <> invalid then return false
        if a.Count() <> b.Count() then return false
        for i = 0 to a.Count() - 1
            if not tkSame(a[i], b[i]) then return false
        end for
        return true
    end if
    if GetInterface(a, "ifAssociativeArray") <> invalid
        if GetInterface(b, "ifAssociativeArray") = invalid then return false
        ' Counted by hand: a key named "count" would hide a.Count().
        aCount = 0
        for each key in a
            aCount = aCount + 1
            if not b.DoesExist(key) then return false
            if not tkSame(a[key], b[key]) then return false
        end for
        bCount = 0
        for each key in b
            bCount = bCount + 1
        end for
        return aCount = bCount
    end if
    return false
end function

function tkShow(value as dynamic) as string
    if value = invalid then return "invalid"
    if GetInterface(value, "ifString") <> invalid then return """" + value + """"
    if GetInterface(value, "ifBoolean") <> invalid
        if value then return "true"
        return "false"
    end if
    if tkIsNumber(value) then return Str(value).Trim()
    if GetInterface(value, "ifArray") <> invalid or GetInterface(value, "ifAssociativeArray") <> invalid
        text = FormatJson(value)
        if text <> "" then return text
    end if
    return "<" + type(value) + ">"
end function

' A stand-in for roRegistrySection: the same methods, in memory.
function tkRegistrySection() as object
    return {
        data: {},
        flushes: 0,
        Exists: function(key as string) as boolean
            return m.data.DoesExist(key)
        end function,
        Read: function(key as string) as string
            if not m.data.DoesExist(key) then return ""
            return m.data[key]
        end function,
        Write: function(key as string, value as string) as boolean
            m.data[key] = value
            return true
        end function,
        Delete: function(key as string) as boolean
            return m.data.Delete(key)
        end function,
        Flush: function() as boolean
            m.flushes = m.flushes + 1
            return true
        end function,
        GetKeyList: function() as object
            return m.data.Keys()
        end function
    }
end function

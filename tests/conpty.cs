// Test-only Windows ConPTY host and minimal screen model for Ratatui output.
// Uses only Windows/.NET; no native module or terminal package is installed.
// ConPTY API: https://learn.microsoft.com/windows/console/creating-a-pseudoconsole-session
using System;
using System.IO;
using System.Text;
using System.Runtime.InteropServices;
using System.Threading;
using Microsoft.Win32.SafeHandles;

public sealed class DebugTuiTerminal : IDisposable {
    [StructLayout(LayoutKind.Sequential)] struct Coord { public short X, Y; public Coord(short x, short y) { X=x; Y=y; } }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] struct StartupInfo {
        public int cb; public string reserved, desktop, title; public int x,y,xsize,ysize,xcount,ycount,fill,flags; public short show,reserved2; public IntPtr reservedPtr,input,output,error;
    }
    [StructLayout(LayoutKind.Sequential)] struct StartupInfoEx { public StartupInfo Startup; public IntPtr Attributes; }
    [StructLayout(LayoutKind.Sequential)] struct ProcessInfo { public IntPtr Process, Thread; public int Id, ThreadId; }
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool CreatePipe(out IntPtr read, out IntPtr write, IntPtr attributes, int size);
    [DllImport("kernel32.dll")] static extern int CreatePseudoConsole(Coord size, IntPtr input, IntPtr output, uint flags, out IntPtr console);
    [DllImport("kernel32.dll")] static extern void ClosePseudoConsole(IntPtr console);
    [DllImport("kernel32.dll")] static extern int ResizePseudoConsole(IntPtr console, Coord size);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr list, int count, int flags, ref IntPtr bytes);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, IntPtr attribute, IntPtr value, IntPtr size, IntPtr previous, IntPtr returned);
    [DllImport("kernel32.dll")] static extern void DeleteProcThreadAttributeList(IntPtr list);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool CreateProcess(string application, StringBuilder command, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr environment, string directory, ref StartupInfoEx startup, out ProcessInfo process);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll")] static extern bool GetExitCodeProcess(IntPtr handle, out uint code);
    [DllImport("kernel32.dll")] static extern bool TerminateProcess(IntPtr handle, uint code);
    [DllImport("kernel32.dll")] static extern IntPtr GetStdHandle(int handle);
    [DllImport("kernel32.dll")] static extern bool SetStdHandle(int handle, IntPtr value);
    readonly object gate = new object(), inputGate = new object();
    FileStream input, output;
    IntPtr console, process, attributes;
    Thread reader;
    char[,] cells;
    int columns, rows, x, y, savedX, savedY, escape;
    bool wrap, disposed;
    readonly StringBuilder sequence = new StringBuilder(), transcript = new StringBuilder();
    public int ProcessId { get; private set; }
    public DebugTuiTerminal(string binary, string arguments, string directory, short width, short height) {
        IntPtr inputRead=IntPtr.Zero,inputWrite=IntPtr.Zero,outputRead=IntPtr.Zero,outputWrite=IntPtr.Zero;
        try {
            if (!CreatePipe(out inputRead,out inputWrite,IntPtr.Zero,0) || !CreatePipe(out outputRead,out outputWrite,IntPtr.Zero,0)) throw new Exception("CreatePipe: " + Marshal.GetLastWin32Error());
            int error = CreatePseudoConsole(new Coord(width,height),inputRead,outputWrite,0,out console);
            if (error!=0) throw new Exception("CreatePseudoConsole HRESULT " + error);
            input = new FileStream(new SafeFileHandle(inputWrite,true),FileAccess.Write); inputWrite=IntPtr.Zero;
            output = new FileStream(new SafeFileHandle(outputRead,true),FileAccess.Read); outputRead=IntPtr.Zero;
            IntPtr size=IntPtr.Zero;
            InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref size);
            attributes=Marshal.AllocHGlobal(size);
            if (!InitializeProcThreadAttributeList(attributes,1,0,ref size)) throw new Exception("Initialize attributes failed");
            if (!UpdateProcThreadAttribute(attributes,0,new IntPtr(0x20016),console,new IntPtr(IntPtr.Size),IntPtr.Zero,IntPtr.Zero)) throw new Exception("ConPTY attribute failed");
            StartupInfoEx info=new StartupInfoEx(); info.Startup.cb=Marshal.SizeOf(typeof(StartupInfoEx)); info.Attributes=attributes;
            ProcessInfo pi;
            ResizeScreen(width,height);
            reader=new Thread(ReadOutput); reader.IsBackground=true; reader.Start();
            // A CLI test host may have redirected standard handles. Prevent those
            // from being copied into the client instead of its ConPTY handles.
            IntPtr stdin=GetStdHandle(-10),stdout=GetStdHandle(-11),stderr=GetStdHandle(-12);
            try {
                SetStdHandle(-10,IntPtr.Zero); SetStdHandle(-11,IntPtr.Zero); SetStdHandle(-12,IntPtr.Zero);
                if (!CreateProcess(binary,new StringBuilder("\""+binary+"\" "+arguments),IntPtr.Zero,IntPtr.Zero,false,0x80000,IntPtr.Zero,directory,ref info,out pi)) throw new Exception("CreateProcess: " + Marshal.GetLastWin32Error());
            } finally { SetStdHandle(-10,stdin); SetStdHandle(-11,stdout); SetStdHandle(-12,stderr); }
            process=pi.Process; ProcessId=pi.Id; CloseHandle(pi.Thread);
        } catch { Dispose(); throw; }
        finally { foreach (IntPtr handle in new []{inputRead,inputWrite,outputRead,outputWrite}) if(handle!=IntPtr.Zero) CloseHandle(handle); }
    }
    void ReadOutput() {
        try {
            using (StreamReader stream=new StreamReader(output,new UTF8Encoding(false),false,4096,true)) {
                char[] buffer=new char[4096]; int count;
                while ((count=stream.Read(buffer,0,buffer.Length))>0) lock(gate) {
                    transcript.Append(buffer,0,count);
                    for(int i=0;i<count;i++) Feed(buffer[i]);
                }
            }
        } catch (IOException) { } catch (ObjectDisposedException) { }
    }
    void Clear() { for(int row=0;row<rows;row++) for(int col=0;col<columns;col++) cells[row,col]=' '; }
    void ResizeScreen(int width,int height) { lock(gate) { columns=width; rows=height; cells=new char[rows,columns]; x=y=0; wrap=false; Clear(); } }
    public void Resize(short width,short height) { ResizeScreen(width,height); if(ResizePseudoConsole(console,new Coord(width,height))!=0) throw new Exception("ConPTY resize failed"); }
    void Feed(char ch) {
        if(escape==3) { if(ch=='\x07') escape=0; else if(ch=='\x1b') escape=4; return; }
        if(escape==4) { escape=ch=='\\'?0:3; return; }
        if(escape==1) { escape=0; if(ch=='[') { escape=2; sequence.Clear(); } else if(ch==']') escape=3; else if(ch=='7') { savedX=x; savedY=y; } else if(ch=='8') { x=savedX; y=savedY; } return; }
        if(escape==2) { if(ch>='@'&&ch<='~') { Execute(ch,sequence.ToString()); escape=0; } else sequence.Append(ch); return; }
        if(ch=='\x1b') { escape=1; return; }
        if(ch=='\r') { x=0; wrap=false; return; }
        if(ch=='\n') { y=Math.Min(rows-1,y+1); wrap=false; return; }
        if(ch=='\b') { x=Math.Max(0,x-1); wrap=false; return; }
        if(ch=='\t') { x=Math.Min(columns-1,((x/8)+1)*8); return; }
        if(char.IsControl(ch) || char.IsLowSurrogate(ch)) return;
        if(wrap) { x=0; y=Math.Min(rows-1,y+1); wrap=false; }
        if(y>=0&&y<rows&&x>=0&&x<columns) cells[y,x]=ch;
        int width=(ch>=0x2e80&&ch<=0x9fff)||(ch>=0xff01&&ch<=0xff60)?2:1;
        for(int i=1;i<width&&x+i<columns;i++) cells[y,x+i]=' ';
        x+=width; if(x>=columns) { x=columns-1; wrap=true; }
    }
    void Execute(char command,string raw) {
        string clean=raw.TrimStart('?','>','!'); string[] parts=clean.Split(';'); int[] values=new int[parts.Length];
        for(int i=0;i<parts.Length;i++) int.TryParse(parts[i],out values[i]);
        int n=values.Length>0&&values[0]>0?values[0]:1;
        switch(command) {
            case 'H': case 'f': y=Math.Max(0,Math.Min(rows-1,n-1)); x=Math.Max(0,Math.Min(columns-1,values.Length>1&&values[1]>0?values[1]-1:0)); wrap=false; break;
            case 'A': y=Math.Max(0,y-n); wrap=false; break;
            case 'B': y=Math.Min(rows-1,y+n); wrap=false; break;
            case 'C': x=Math.Min(columns-1,x+n); wrap=false; break;
            case 'D': x=Math.Max(0,x-n); wrap=false; break;
            case 'G': x=Math.Min(columns-1,n-1); wrap=false; break;
            case 'd': y=Math.Min(rows-1,n-1); wrap=false; break;
            case 'J': if(values[0]==2||values[0]==3) Clear(); else for(int row=0;row<rows;row++) for(int col=0;col<columns;col++) if(values[0]==0?(row>y||row==y&&col>=x):(row<y||row==y&&col<=x)) cells[row,col]=' '; break;
            case 'K': for(int col=0;col<columns;col++) if(values[0]==2||(values[0]==0?col>=x:col<=x)) cells[y,col]=' '; break;
            case 'X': for(int col=x;col<Math.Min(columns,x+n);col++) cells[y,col]=' '; break;
            case 's': savedX=x; savedY=y; break;
            case 'u': x=savedX; y=savedY; wrap=false; break;
            case 'h': if(raw=="?1049") { x=y=0; wrap=false; Clear(); } break;
            case 'n': if(raw=="6") Send("\x1b["+(y+1)+";"+(x+1)+"R"); break;
        }
    }
    public void Send(string text) { lock(inputGate) { byte[] bytes=Encoding.UTF8.GetBytes(text); input.Write(bytes,0,bytes.Length); input.Flush(); } }
    public string Screen() { lock(gate) { StringBuilder text=new StringBuilder(); for(int row=0;row<rows;row++) { for(int col=0;col<columns;col++) text.Append(cells[row,col]); text.Append('\n'); } return text.ToString(); } }
    public string Transcript() { lock(gate) return transcript.ToString(); }
    public bool WaitForExit(int milliseconds) { return process!=IntPtr.Zero&&WaitForSingleObject(process,(uint)milliseconds)==0; }
    public uint ExitCode() { uint code; GetExitCodeProcess(process,out code); return code; }
    public void Dispose() {
        if(disposed) return; disposed=true;
        if(process!=IntPtr.Zero) { if(!WaitForExit(1000)) TerminateProcess(process,1); WaitForExit(3000); }
        if(console!=IntPtr.Zero) { ClosePseudoConsole(console); console=IntPtr.Zero; }
        if(input!=null) input.Dispose(); if(output!=null) output.Dispose();
        if(process!=IntPtr.Zero) { CloseHandle(process); process=IntPtr.Zero; }
        if(attributes!=IntPtr.Zero) { DeleteProcThreadAttributeList(attributes); Marshal.FreeHGlobal(attributes); attributes=IntPtr.Zero; }
    }
}

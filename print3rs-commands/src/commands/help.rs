static FULL_HELP: &str = "    
Anything entered not matching one of the following commands is uppercased and sent to
the printer for it to interpret.

Some commands cannot be ran until a printer is connected.

Multiple Gcodes can be sent on the same line by separating with ';'.

Arguments with ? are optional.

Available commands:
help         <command?>       display this message or details for specified command
version                       display version
clear                         clear all text on the screen
printerinfo                   display any information found about the connected printer
print        <file>           send gcodes from file to printer
log          <name> <pattern> begin logging parsed output from printer
repeat       <name> <gcodes>  run the given gcodes in a loop until stop
stop         <name>           stop an active print, log, or repeat
macro        <name> <gcodes>  make an alias for a set of gcodes
delmacro     <name>           remove an existing alias for set of gcodes
macros                        list existing command aliases and contents           
connect      <proto?> <args?> connect to a device using protocol and args, or attempt to autoconnect
disconnect                    disconnect from printer
quit                          exit program
\n";

static PRINT_HELP: &str = "print: execute every line of G-code sequentially from the given file. The print job is added as a task which runs in the background with the filename as the task name. Other commands can be sent while a print is running, and a print can be stopped at any time with `stop`\n";
static LOG_HELP: &str = "log: begin logging the specified pattern from the printer into a csv with the `name` given. This operation runs in the background and is added as a task which can be stopped with `stop`. The pattern given will be used to parse the logs, with values wrapped in `{}` being given a column of whatever is between the `{}`, and pulling a number in its place. If your pattern needs to include a literal `{` or `}`, double them up like `{{` or `}}` to have the parser read it as just a `{` or `}` in the output.\n";
static REPEAT_HELP: &str = "repeat: repeat the given Gcodes (separated by gcode comment character `;`) in a loop until stopped. \n";
static STOP_HELP: &str = "stop: stops a task running in the background. All background tasks are required to have a name, thus this command can be used to stop them. Tasks can also stop themselves if they fail or can complete, after which running this will do nothing.\n";
static CONNECT_HELP: &str = "connect: Manually connect to a printer by specifying a protocol and some arguments. Arguments depend on protocol. For serial connection specify its path and optionally its baudrate. On windows this looks like `connect serial COM3 115200`, on linux more like `connect serial /dev/tty/ACM0 250000`. This does not test if the printer is capable of responding to messages, it will only open the port. Specifying no arguments will attempt autoconnection using serial.\n";
static DISCONNECT_HELP: &str = "disconnect: disconnect from the currently connected printer. All active tasks will be stopped\n";
static MACRO_HELP: &str = "create a case-insensitve alias to some set of gcodes, even containing other macros recursively to build up complex sets of builds with a single word. Macro names cannot be a single uppercase letter followed by a number, e.g. H105, to avoid conflict with Gcodes. Names can have any mix of alphanumeric, -, ., and _ characters. Commands in a macro are separated by ';', and macros can be used anywhere Gcodes are passed, including repeat commands and sends.\n";

pub fn help(command: &str) -> &'static str {
    let command = command.trim();

    match command {
        "print" => PRINT_HELP,
        "log" => LOG_HELP,
        "repeat" => REPEAT_HELP,
        "stop" => STOP_HELP,
        "connect" => CONNECT_HELP,
        "disconnect" => DISCONNECT_HELP,
        "macro" => MACRO_HELP,
        _ => FULL_HELP,
    }
}

#[cfg(test)]
#[test]
fn test_help() {
    assert_eq!(help(""), FULL_HELP);
    assert_eq!(help("print"), PRINT_HELP);
    assert_eq!(help("log"), LOG_HELP);
    assert_eq!(help("repeat"), REPEAT_HELP);
    assert_eq!(help("stop"), STOP_HELP);
    assert_eq!(help("connect"), CONNECT_HELP);
    assert_eq!(help("disconnect"), DISCONNECT_HELP);
    assert_eq!(help("macro"), MACRO_HELP);
}

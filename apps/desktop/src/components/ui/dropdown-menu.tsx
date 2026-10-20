import * as React from "react";
import {DropdownMenu as Primitive} from "radix-ui";
import {cn} from "@/lib/utils";
export const DropdownMenu=Primitive.Root;
export const DropdownMenuTrigger=Primitive.Trigger;
export const DropdownMenuGroup=Primitive.Group;
export function DropdownMenuContent({className,sideOffset=6,...props}:React.ComponentProps<typeof Primitive.Content>){
 return <Primitive.Portal><Primitive.Content sideOffset={sideOffset} className={cn("z-50 min-w-44 rounded-lg border bg-popover p-1 text-popover-foreground shadow-md",className)} {...props}/></Primitive.Portal>;
}
export function DropdownMenuItem({className,...props}:React.ComponentProps<typeof Primitive.Item>){
 return <Primitive.Item className={cn("flex cursor-default items-center gap-2 rounded-md px-3 py-2 text-sm outline-none data-[highlighted]:bg-accent data-[highlighted]:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50 [&_svg]:size-4",className)} {...props}/>;
}

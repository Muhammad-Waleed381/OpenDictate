import * as React from "react"
import { Input as InputPrimitive } from "@base-ui/react/input"

import { cn } from "@/lib/utils"

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <InputPrimitive
      type={type}
      data-slot="input"
      className={cn(
        "h-9 w-full min-w-0 rounded-none border-2 border-input bg-background px-3 py-1 text-sm shadow-none transition-all duration-150 ease-spring outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-bold file:uppercase file:tracking-wide file:text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:shadow-brutal disabled:pointer-events-none disabled:cursor-not-allowed disabled:bg-muted disabled:opacity-50 aria-invalid:border-destructive aria-invalid:shadow-brutal",
        className
      )}
      {...props}
    />
  )
}

export { Input }

import { mergeProps } from "@base-ui/react/merge-props"
import { useRender } from "@base-ui/react/use-render"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "group/badge inline-flex h-6 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-none border-2 border-border px-2 py-0.5 text-xs font-bold uppercase tracking-wide whitespace-nowrap transition-all duration-150 ease-spring focus-visible:ring-[3px] focus-visible:ring-ring has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 aria-invalid:border-destructive [&>svg]:pointer-events-none [&>svg]:size-3!",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground shadow-[2px_2px_0_0_var(--od-shadow)] [a]:hover:shadow-none [a]:hover:translate-x-[2px] [a]:hover:translate-y-[2px]",
        secondary:
          "bg-secondary text-secondary-foreground shadow-[2px_2px_0_0_var(--od-shadow)] [a]:hover:shadow-none [a]:hover:translate-x-[2px] [a]:hover:translate-y-[2px]",
        destructive:
          "bg-primary text-primary-foreground shadow-[2px_2px_0_0_var(--od-shadow)] [a]:hover:shadow-none [a]:hover:translate-x-[2px] [a]:hover:translate-y-[2px]",
        outline:
          "border-border bg-background text-foreground shadow-[2px_2px_0_0_var(--od-shadow)] [a]:hover:bg-accent",
        ghost:
          "border-border bg-transparent text-foreground shadow-none hover:bg-accent",
        link: "border-transparent bg-transparent text-primary underline-offset-4 hover:underline shadow-none",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

function Badge({
  className,
  variant = "default",
  render,
  ...props
}: useRender.ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return useRender({
    defaultTagName: "span",
    props: mergeProps<"span">(
      {
        className: cn(badgeVariants({ variant }), className),
      },
      props
    ),
    render,
    state: {
      slot: "badge",
      variant,
    },
  })
}

export { Badge, badgeVariants }

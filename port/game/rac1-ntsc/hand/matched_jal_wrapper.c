/* SPDX-License-Identifier: GPL-3.0-or-later */
/* FUN_L00_002371e0 (hud_counter_update) as the port runs it. The catalogue folds 106 places into
 * this name. Each place is the same small wrapper: it calls one function and passes its argument
 * registers on unchanged. But each place calls its own function (a HUD update,
 * mark_moby_for_removal, fast_tween_color, ...). The matching C says (void), so when this C ran
 * (an imported class from another level, or a mod hook), the callee got 0 in a0. Here the
 * wrapper takes the registers its callees read (a0 to a3, f12) and passes them to the function
 * its own copy calls (call_copies.tsv). Not a PS2 match; the decomp's C stays as it is. */
extern unsigned long jal_wrapper_callee(unsigned long, unsigned long, unsigned long, unsigned long,
                                        float) __asm__("FUN_L00_00235ea0");

unsigned long hud_counter_update(unsigned long a0, unsigned long a1, unsigned long a2,
                                 unsigned long a3, float f12) {
    return jal_wrapper_callee(a0, a1, a2, a3, f12);
}
